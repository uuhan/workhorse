use std::collections::HashMap;
use std::sync::Arc;

use crate::db::entity::prelude::{SshAuth, User};
use crate::key::KEY;
use crate::prelude::*;
use anyhow::Context;
use russh::keys::ssh_key::{Certificate, PublicKey};
use russh::{server::*, MethodSet};
use russh::{Channel, ChannelId, Sig};
use sea_orm::{DatabaseConnection, EntityTrait, ModelTrait};
use tokio::process::Command;
use tokio::sync::Mutex;

mod handle;
use handle::ChannelHandle;

struct AppServer {
    /// 一些共享数据
    clients: Arc<Mutex<HashMap<ChannelId, ChannelHandle>>>,
    /// 数据库连接
    db: DatabaseConnection,
    /// 当前 Client 的 ChannelHandle
    handle: Option<ChannelHandle>,
    /// 当前 Client 的用户名
    action: String,
}

impl Clone for AppServer {
    fn clone(&self) -> Self {
        Self {
            clients: self.clients.clone(),
            db: DB.clone(),
            handle: None,
            action: String::new(),
        }
    }
}

impl AppServer {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            handle: None,
            action: String::new(),
            db: DB.clone(),
        }
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
    /// 创建新连接
    fn new_client(&mut self, peer: Option<std::net::SocketAddr>) -> Self {
        tracing::info!("New Client: {:?}", peer);
        self.clone()
    }

    /// 处理会话错误
    fn handle_session_error(&mut self, error: <Self::Handler as Handler>::Error) {
        tracing::error!("会话错误: {:?}", error);
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
        self.handle.replace(ChannelHandle::from(channel, session));

        Ok(true)
    }

    /// Check authentication using the "password" method. Russh
    /// makes sure rejection happens in time
    /// `config.auth_rejection_time`, except if this method takes more
    /// than that.
    async fn auth_password(&mut self, action: &str, _password: &str) -> Result<Auth, Self::Error> {
        tracing::info!("尝试使用密码执行: {action}");
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

        let Some(sa) = SshAuth::find_by_id((pk.algorithm().to_string(), data.to_owned()))
            .one(&self.db)
            .await?
        else {
            tracing::error!("公钥未记录: ({} {})", pk.algorithm().to_string(), data);
            return Ok(Auth::Reject {
                proceed_with_methods: Some(MethodSet::PUBLICKEY),
            });
        };

        let Some(user) = sa.find_related(User).one(&self.db).await? else {
            tracing::error!("公钥未授权: ({} {})", pk.algorithm().to_string(), data);
            return Ok(Auth::Reject {
                proceed_with_methods: Some(MethodSet::PUBLICKEY),
            });
        };

        self.action = action.to_string();

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
    // async fn auth_succeeded(&mut self, session: &mut Session) -> Result<(), Self::Error> {
    //     tracing::info!("Auth Succeeded");
    //     Ok(())
    // }

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
        channel: ChannelId,
        key: &str,
        value: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("[{channel}] ssh env request: {key}={value}");
        session.channel_success(channel)?;
        Ok(())
    }

    /// The client requests a shell.
    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("ssh shell request");
        session.channel_success(channel)?;
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
        match &self.action[..] {
            "git" => {
                // git clone ssh://git@127.0.0.1:2222/repos/a
                // git-upload-pack '/repos/a'
                let command = String::from_utf8_lossy(data);
                tracing::info!("GIT: {}", command);
                let handle = self.handle.take().context("FIXME: no handle").unwrap();

                if command.starts_with("git-upload-pack") {
                    tokio::spawn(async move {
                        if let Err(err) = handle
                            .exec(Command::new("git").arg("upload-pack").arg("./repos/a"))
                            .await
                        {
                            tracing::error!("Exec Error: {}", err);
                        }
                    });
                } else if command.starts_with("git-receive-pack") {
                    tokio::spawn(async move {
                        if let Err(err) = handle
                            .exec(Command::new("git").arg("receive-pack").arg("./repos/a"))
                            .await
                        {
                            tracing::error!("Exec Error: {}", err);
                        }
                    });
                }
            }
            "cmd" => {
                let command = String::from_utf8_lossy(data).to_string();
                tracing::info!("CMD: {}", command);
                let handle = self.handle.take().context("FIXME: no handle").unwrap();
                tokio::spawn(async move {
                    #[cfg(windows)]
                    if let Err(err) = handle
                        .exec(Command::new("cmd.exe").arg("/C").arg(command))
                        .await
                    {
                        tracing::error!("command failed: {}", err);
                    }
                    #[cfg(not(windows))]
                    if let Err(err) = handle.exec(Command::new("sh").arg("-c").arg(command)).await {
                        tracing::error!("command failed: {}", err);
                    }
                });
            }
            other => {
                tracing::warn!("Unknown action: {other}");
                session.channel_failure(channel_id)?;
                return Ok(());
            }
        }

        session.channel_success(channel_id)?;

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
        tracing::debug!("Recv Data: {}", data.len());
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

impl Drop for AppServer {
    fn drop(&mut self) {
        tracing::info!("Drop AppServer");
    }
}

pub async fn run() -> HorseResult<()> {
    let mut server = AppServer::new();
    server.run().await.expect("Failed running server");
    Ok(())
}
