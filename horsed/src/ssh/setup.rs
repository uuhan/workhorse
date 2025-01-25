use crate::db::entity::{
    prelude::{SshPk, User},
    ssh_pk, user,
};
use crate::prelude::DB;
use russh::keys::ssh_key::PublicKey;
use russh::{server::*, MethodSet};
use sea_orm::ActiveValue::{self, NotSet, Set, Unchanged};
use sea_orm::{EntityTrait, ModelTrait};

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
    type Error = anyhow::Error;

    async fn auth_publickey_offered(
        &mut self,
        action: &str,
        pk: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        tracing::info!("Auth Publickey Offered: {}, {:?}", action, pk.to_openssh());
        Ok(Auth::Accept)
    }

    async fn auth_publickey(&mut self, user: &str, pk: &PublicKey) -> anyhow::Result<Auth> {
        let alg = pk.algorithm();
        #[allow(deprecated)]
        let key = base64::encode(&pk.to_bytes()?);
        let user_name = user;

        let conn = DB.clone();

        let user = user::ActiveModel {
            name: Set(user.to_string()),
            ..Default::default()
        };

        let ir = User::insert(user).exec(&conn).await?;
        let id = ir.last_insert_id;

        let auth = ssh_pk::ActiveModel {
            alg: Set(alg.to_string()),
            key: Set(key),
            user_id: Set(id),
            ..Default::default()
        };

        tracing::info!("Setup As: {}", user_name);

        Ok(Auth::Accept)
    }
}
