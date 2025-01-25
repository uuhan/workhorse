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
use futures::future::FutureExt;
use russh::keys::ssh_key::PublicKey;
use russh::ChannelId;
use russh::{server::*, MethodSet};
use sea_orm::{
    ActiveModelTrait,
    ActiveValue::Set,
    ColumnTrait, QueryFilter, TransactionTrait, {EntityTrait, ModelTrait},
};

struct SetupServer {}

impl Server for SetupServer {
    type Handler = Self;

    fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> Self {
        SetupServer {}
    }

    fn handle_session_error(&mut self, _error: <Self::Handler as Handler>::Error) {}
}

#[async_trait::async_trait]
impl Handler for SetupServer {
    type Error = HorseError;

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

        let user_name = user.to_string();
        if let Err(err) = conn
            .transaction::<_, (), HorseError>(move |txn| {
                async move {
                    let name = user_name.clone();
                    let user = user::ActiveModel {
                        name: Set(name),
                        ..Default::default()
                    };

                    user.save(txn).await?;

                    let res = User::find()
                        .filter(user::Column::Name.eq(user_name))
                        .one(txn)
                        .await?
                        .context("bad txn")?;

                    let auth = ssh_pk::ActiveModel {
                        alg: Set(alg.to_string()),
                        key: Set(key),
                        user_id: Set(res.id),
                    };

                    auth.save(txn).await?;
                    Ok(())
                }
                .boxed()
            })
            .await
        {
            tracing::error!("DB ERROR: {err:?}");
        }

        tracing::info!("Setup As: {}", user);

        Ok(Auth::Accept)
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        command: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }
}
