/// SETUP MODE:
///
/// THIS CODE IS NOT USED IN THE PRODUCTION ENVIRONMENT. IT IS ONLY USED TO SET UP THE SERVER.
///
use crate::db::entity::{
    prelude::{SshPk, User},
    ssh_pk, user,
};
use crate::prelude::*;
use anyhow::Context;
use colored::Colorize;
use futures::future::FutureExt;
use rand_core::OsRng;
use russh::{keys::ssh_key::PublicKey, CryptoVec};
use russh::{server::*, MethodSet};
use russh::{Channel, ChannelId};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, QueryFilter, TransactionTrait, {EntityTrait, ModelTrait},
};
use stable::task::SpawnEssentialTaskHandle;
use std::sync::Arc;

#[derive(Clone)]
struct SetupServer {
    pub handle: SpawnEssentialTaskHandle,
    pub in_danger: bool,
}

impl SetupServer {
    pub fn new(handle: SpawnEssentialTaskHandle, in_danger: bool) -> Self {
        Self { handle, in_danger }
    }

    pub async fn run(&mut self) -> HorseResult<()> {
        let config = Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(3600)),
            auth_rejection_time: std::time::Duration::from_secs(1),
            auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
            auth_banner: None,
            keys: vec![russh_keys::PrivateKey::random(
                &mut OsRng,
                russh_keys::Algorithm::Ed25519,
            )?],
            keepalive_interval: Some(std::time::Duration::from_secs(5)),
            ..Default::default()
        };

        self.run_on_address(Arc::new(config), ("0.0.0.0", 2223))
            .await?;

        Ok(())
    }
}

impl Server for SetupServer {
    type Handler = Self;

    fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> Self {
        self.clone()
    }

    fn handle_session_error(&mut self, _error: <Self::Handler as Handler>::Error) {}
}

#[async_trait::async_trait]
impl Handler for SetupServer {
    type Error = HorseError;

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        session: &mut Session,
    ) -> HorseResult<bool> {
        Ok(true)
    }

    async fn auth_publickey_offered(
        &mut self,
        action: &str,
        pk: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        tracing::info!("PK Offered: [{}] {:?}", action, pk.to_openssh());
        Ok(Auth::Accept)
    }

    async fn auth_publickey(&mut self, user: &str, pk: &PublicKey) -> HorseResult<Auth> {
        let alg = pk.algorithm();
        #[allow(deprecated)]
        let key = base64::encode(&pk.to_bytes()?);

        let conn = DB.clone();

        // Check if the key is already in the database
        if let Some(pk) = SshPk::find_by_id((alg.to_string(), key.clone()))
            .one(&conn)
            .await?
        {
            // Check if there is some user already associated with the key
            if let Some(user) = pk.find_related(User).one(&conn).await? {
                tracing::info!("User already exists: {}", user.name);
                return Ok(Auth::Accept);
            } else {
                // If there is no user associated with the key, but it is impossible
                tracing::warn!("Key without user: {}", pk.user_id);
                return Ok(Auth::Reject {
                    proceed_with_methods: Some(MethodSet::PUBLICKEY),
                });
            }
        }

        let user_name = user.to_string();
        if let Err(err) = conn
            .transaction::<_, (), HorseError>(move |txn| {
                async move {
                    let name = user_name.clone();
                    let user = if let Some(user) = User::find()
                        .filter(user::Column::Name.eq(&name))
                        .one(txn)
                        .await?
                    {
                        user.into()
                    } else {
                        user::ActiveModel {
                            name: Set(name),
                            ..Default::default()
                        }
                        .save(txn)
                        .await
                        .context(format!("create user: {}", user_name))?
                    };

                    let auth = ssh_pk::ActiveModel {
                        alg: Set(alg.to_string()),
                        key: Set(key),
                        user_id: user.id,
                    };

                    if let Err(err) = auth.insert(txn).await {
                        tracing::warn!("{:?}", err);
                    }

                    Ok(())
                }
                .boxed()
            })
            .await
        {
            tracing::error!("DB ERROR: {err:?}");
        }

        Ok(Auth::Accept)
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("Shell Request: {:?}", channel);
        session.close(channel)?;

        if !self.in_danger {
            self.handle.exit();
        }

        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        command: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("Exec Request: {:?}", channel);
        session.channel_success(channel)?;
        Ok(())
    }
}

pub async fn run(handle: SpawnEssentialTaskHandle, in_danger: bool) -> HorseResult<()> {
    SetupServer::new(handle, in_danger).run().await
}
