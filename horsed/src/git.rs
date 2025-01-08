use std::collections::HashMap;
use std::sync::Arc;

use crate::db::entity::prelude::{SshAuth, User};
use crate::key::KEY;
use crate::prelude::*;
use russh::keys::ssh_key::{Certificate, PublicKey};
use russh::server::*;
use russh::{Channel, ChannelId, Pty, Sig};
use sea_orm::{DatabaseConnection, EntityTrait, ModelTrait};
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;

#[derive(Clone)]
struct AppServer {
    clients: Arc<Mutex<HashMap<ChannelId, Channel<Msg>>>>,
    db: DatabaseConnection,
}

impl AppServer {
    pub async fn new() -> HorseResult<Self> {
        let db = crate::db::connect().await?;
        Ok(Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            db,
        })
    }

    pub async fn get_channel(&mut self, channel_id: ChannelId) -> Channel<Msg> {
        let mut clients = self.clients.lock().await;
        clients.remove(&channel_id).unwrap()
    }

    pub async fn run(&mut self) -> HorseResult<()> {
        let config = Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(3600)),
            auth_rejection_time: std::time::Duration::from_secs(3),
            auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
            keys: vec![KEY.clone()],
            ..Default::default()
        };

        self.run_on_address(Arc::new(config), ("0.0.0.0", 2222))
            .await?;
        Ok(())
    }
}

impl Server for AppServer {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        self.clone()
    }
}

#[async_trait::async_trait]
impl Handler for AppServer {
    type Error = HorseError;

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        session: &mut Session,
    ) -> HorseResult<bool> {
        let mut clients = self.clients.lock().await;
        clients.insert(channel.id(), channel);

        Ok(true)
    }

    /// Check authentication using the "password" method. Russh
    /// makes sure rejection happens in time
    /// `config.auth_rejection_time`, except if this method takes more
    /// than that.
    async fn auth_password(&mut self, user: &str, _password: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Reject {
            proceed_with_methods: None,
        })
    }

    /// Check authentication using the "publickey" method. This method
    /// should just check whether the public key matches the
    /// authorized ones. Russh then checks the signature. If the key
    /// is unknown, or the signature is invalid, Russh guarantees
    /// that rejection happens in constant time
    /// `config.auth_rejection_time`, except if this method takes more
    /// time than that.
    async fn auth_publickey_offered(
        &mut self,
        action: &str,
        pk: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        tracing::info!("Auth Publickey Offered: {}, {:?}", action, pk.to_openssh());
        Ok(Auth::Accept)
    }

    /// Check authentication using the "publickey" method. This method
    /// is called after the signature has been verified and key
    /// ownership has been confirmed.
    /// Russh guarantees that rejection happens in constant time
    /// `config.auth_rejection_time`, except if this method takes more
    /// time than that.
    async fn auth_publickey(&mut self, action: &str, pk: &PublicKey) -> HorseResult<Auth> {
        #[allow(deprecated)]
        let data = base64::encode(&pk.to_bytes()?);

        let Some(sa) = SshAuth::find_by_id((pk.algorithm().to_string(), data))
            .one(&self.db)
            .await?
        else {
            return Ok(Auth::Reject {
                proceed_with_methods: None,
            });
        };

        let Some(user) = sa.find_related(User).one(&self.db).await? else {
            return Ok(Auth::Reject {
                proceed_with_methods: None,
            });
        };

        tracing::info!("Action: {action}, Login As: {}", user.name);
        Ok(Auth::Accept)
    }

    /// Check authentication using an OpenSSH certificate. This method
    /// is called after the signature has been verified and key
    /// ownership has been confirmed.
    /// Russh guarantees that rejection happens in constant time
    /// `config.auth_rejection_time`, except if this method takes more
    /// time than that.
    async fn auth_openssh_certificate(
        &mut self,
        _user: &str,
        _certificate: &Certificate,
    ) -> Result<Auth, Self::Error> {
        Ok(Auth::Reject {
            proceed_with_methods: None,
        })
    }

    /// Check authentication using the "keyboard-interactive"
    /// method. Russh makes sure rejection happens in time
    /// `config.auth_rejection_time`, except if this method takes more
    /// than that.
    async fn auth_keyboard_interactive(
        &mut self,
        _user: &str,
        _submethods: &str,
        _response: Option<Response<'async_trait>>,
    ) -> Result<Auth, Self::Error> {
        Ok(Auth::Reject {
            proceed_with_methods: None,
        })
    }

    /// Called when authentication succeeds for a session.
    async fn auth_succeeded(&mut self, session: &mut Session) -> Result<(), Self::Error> {
        Ok(())
    }

    /// The client requests an X11 connection.
    async fn x11_request(
        &mut self,
        _channel: ChannelId,
        _single_connection: bool,
        _x11_auth_protocol: &str,
        _x11_auth_cookie: &str,
        _x11_screen_number: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // session.channel_success(channel);
        Ok(())
    }

    /// The client wants to set the given environment variable. Check
    /// these carefully, as it is dangerous to allow any variable
    /// environment to be set.
    async fn env_request(
        &mut self,
        _channel: ChannelId,
        _variable_name: &str,
        _variable_value: &str,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // session.channel_success(channel);
        Ok(())
    }

    /// The client requests a shell.
    async fn shell_request(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // session.channel_success(channel);
        Ok(())
    }

    /// The client sends a command to execute, to be passed to a
    /// shell. Make sure to check the command before doing so.
    async fn exec_request(
        &mut self,
        channel_id: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // git clone ssh://git@127.0.0.1:2222/repos/a
        // git-upload-pack '/repos/a'
        let command = String::from_utf8_lossy(data);
        tracing::info!("EXEC: {}", command);

        let mut channel = self.get_channel(channel_id).await;
        let handle = session.handle();

        if command.starts_with("git-upload-pack") {
            tokio::spawn(async move {
                let mut channel = channel;
                let mut handle = handle;
                let mut git = tokio::process::Command::new("git")
                    .arg("upload-pack")
                    .arg("./repos/a")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()?;
                let mut stdout = git.stdout.take().unwrap();
                let mut stdin = git.stdin.take().unwrap();
                let mut cout = channel.make_writer();

                tokio::spawn(async move {
                    while let Ok(len) = tokio::io::copy(&mut stdout, &mut cout).await {
                        tracing::info!("send data: {}", len);
                        if len == 0 {
                            break;
                        }
                    }
                });

                tokio::spawn(async move {
                    let mut cin = channel.make_reader();
                    while let Ok(len) = tokio::io::copy(&mut cin, &mut stdin).await {
                        tracing::info!("receive data: {}", len);
                        if len == 0 {
                            break;
                        }
                    }
                });

                if let Err(err) = git.wait().await {
                    tracing::error!("git error: {}", err);
                }
                tracing::info!("git exit");
                handle.channel_success(channel_id).await;

                Result::<(), HorseError>::Ok(())
            });
        } else if command.starts_with("git-receive-pack") {
            tokio::spawn(async move {
                let mut channel = channel;
                let mut handle = handle;
                // 处理 git-receive-pack 命令（通常是推送）
                let mut git = tokio::process::Command::new("git")
                    .arg("receive-pack")
                    .arg("./repos/a")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()?;

                let mut stdout = git.stdout.take().unwrap();
                let mut stdin = git.stdin.take().unwrap();
                let mut cout = channel.make_writer();
                tokio::spawn(async move {
                    while let Ok(len) = tokio::io::copy(&mut stdout, &mut cout).await {
                        tracing::info!("send data: {}", len);
                        if len == 0 {
                            break;
                        }
                    }
                });

                tokio::spawn(async move {
                    let mut cin = channel.make_reader();
                    while let Ok(len) = tokio::io::copy(&mut cin, &mut stdin).await {
                        tracing::info!("receive data: {}", len);
                        if len == 0 {
                            break;
                        }
                    }
                });

                if let Err(err) = git.wait().await {
                    tracing::error!("git error: {}", err);
                }
                tracing::info!("git exit");
                handle.channel_success(channel_id).await;

                Result::<(), HorseError>::Ok(())
            });
        }

        session.channel_success(channel_id);

        Ok(())
    }

    /// The client asks to start the subsystem with the given name
    /// (such as sftp).
    async fn subsystem_request(
        &mut self,
        _channel: ChannelId,
        name: &str,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        println!("SUBSYSTEM: {name}");
        // session.channel_success(channel);
        Ok(())
    }

    /// The client requests OpenSSH agent forwarding
    async fn agent_request(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        // session.channel_success(channel);
        Ok(false)
    }

    /// The client is sending a signal (usually to pass to the
    /// currently running process).
    async fn signal(
        &mut self,
        _channel: ChannelId,
        sig: Sig,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("Client Signal: {:?}", sig);
        Ok(())
    }

    async fn data(
        &mut self,
        channel_id: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> HorseResult<()> {
        tracing::info!("Recv Data: {}", data.len());
        Ok(())
    }

    /// 当客户端传输结束时调用。
    async fn channel_eof(
        &mut self,
        channel_id: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("Channel Eof");
        Ok(())
    }

    /// Called when the client closes a channel.
    async fn channel_close(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("Channel Close");
        Ok(())
    }
}

pub async fn run() -> HorseResult<()> {
    let mut server = AppServer::new().await?;
    server.run().await.expect("Failed running server");
    Ok(())
}
