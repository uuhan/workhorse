use std::time::Duration;

#[allow(unused_imports)]
use super::*;
use futures::executor::block_on;
use migration::{Migrator, MigratorTrait};
use rand_core::OsRng;
use rstest::*;
use russh::client::DisconnectReason;
use russh::keys::{Algorithm, PrivateKey};
use russh::{
    client::{self, Handle, Handler},
    key::PrivateKeyWithHashAlg,
    server, Disconnect, Error,
};
use sea_orm::{ConnectOptions, Database, DbConn};
use setup::SetupServer;
use tokio::net::ToSocketAddrs;

type Result<T> = std::result::Result<T, Error>;

struct TestClient {
    handle: Handle<Client>,
}

struct Client {}

#[async_trait::async_trait]
impl Handler for Client {
    type Error = Error;

    async fn check_server_key(&mut self, _pk: &PublicKey) -> Result<bool> {
        Ok(true)
    }

    async fn disconnected(&mut self, reason: DisconnectReason<Self::Error>) -> Result<()> {
        match reason {
            DisconnectReason::ReceivedDisconnect(_) => Ok(()),
            DisconnectReason::Error(e) => Err(e),
        }
    }
}

impl TestClient {
    async fn connect<A: ToSocketAddrs>(
        key: PrivateKey,
        user: impl Into<String>,
        addrs: A,
    ) -> Result<Self> {
        let config = client::Config {
            inactivity_timeout: None,
            keepalive_interval: None,
            ..<_>::default()
        };

        let mut handle = client::connect(Arc::new(config), addrs, Client {}).await?;
        let auth_res = handle
            .authenticate_publickey(
                user.into(),
                PrivateKeyWithHashAlg::new(Arc::new(key), None)?,
            )
            .await?;

        if !auth_res {
            return Err(Error::NotAuthenticated);
        }

        Ok(Self { handle })
    }

    #[allow(unused)]
    async fn close(&mut self) -> anyhow::Result<()> {
        self.handle
            .disconnect(Disconnect::ByApplication, "", "English")
            .await?;
        Ok(())
    }
}

#[fixture]
#[once]
fn key() -> PrivateKey {
    PrivateKey::random(&mut OsRng, Algorithm::Ed25519).unwrap()
}

#[fixture]
#[once]
fn db() -> DbConn {
    // tracing_subscriber::fmt()
    //     .with_env_filter("info,russh=info")
    //     .init();

    let url = "sqlite::memory:";
    let mut opt = ConnectOptions::new(url);
    opt.connect_timeout(Duration::from_secs(8))
        .acquire_timeout(Duration::from_secs(8))
        .idle_timeout(Duration::from_secs(8))
        .max_lifetime(Duration::from_secs(8))
        .sqlx_logging(true)
        .set_schema_search_path("my_schema"); // Setting default PostgreSQL schema

    let db = block_on(Database::connect(opt)).unwrap();

    tracing::info!("migrate");
    block_on(Migrator::up(&db, None)).unwrap();

    db
}

#[rstest]
async fn test_ssh_server_auth(key: &PrivateKey, db: &DbConn) {
    let config = server::Config {
        inactivity_timeout: None,
        auth_rejection_time: std::time::Duration::from_secs(0),
        auth_rejection_time_initial: None,
        keys: vec![key.clone()],
        keepalive_interval: None,
        ..Default::default()
    };

    let tm = TaskManager::default();
    let handle = tm.spawn_essential_handle();
    let mut setup_server = SetupServer::new(handle, db.clone(), true);
    tokio::spawn(async move { setup_server.run(config, ("127.0.0.1", 1223)).await });

    let config = server::Config {
        inactivity_timeout: None,
        auth_rejection_time: std::time::Duration::from_secs(0),
        auth_rejection_time_initial: None,
        keys: vec![key.clone()],
        keepalive_interval: None,
        ..Default::default()
    };

    let mut ssh_server = AppServer::new(db.clone());
    tokio::spawn(async move { ssh_server.run(config, ("127.0.0.1", 1222)).await });

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let ssh = TestClient::connect(key.clone(), "any", ("127.0.0.1", 1223)).await;
    assert!(ssh.is_ok());

    let ssh = TestClient::connect(key.clone(), "ping", ("127.0.0.1", 1222)).await;
    assert!(ssh.is_ok());
}
