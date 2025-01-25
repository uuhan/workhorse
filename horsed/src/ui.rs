use std::collections::HashMap;
use std::sync::Arc;

use crate::db::entity::prelude::{SshPk, User};
use crate::prelude::*;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::{Terminal, TerminalOptions, Viewport};
use russh::keys::ssh_key::{Certificate, PublicKey};
use russh::server::*;
use russh::{Channel, ChannelId, Pty, Sig};
use sea_orm::{DatabaseConnection, EntityTrait, ModelTrait};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::Mutex;

type SshTerminal = Terminal<CrosstermBackend<TerminalHandle>>;

struct App {
    pub counter: usize,
}

impl App {
    pub fn new() -> App {
        Self { counter: 0 }
    }
}

struct TerminalHandle {
    sender: UnboundedSender<Vec<u8>>,
    // The sink collects the data which is finally sent to sender.
    sink: Vec<u8>,
}

impl TerminalHandle {
    async fn start(handle: Handle, channel_id: ChannelId) -> Self {
        let (sender, mut receiver) = unbounded_channel::<Vec<u8>>();
        tokio::spawn(async move {
            while let Some(data) = receiver.recv().await {
                let result = handle.data(channel_id, data.into()).await;
                if result.is_err() {
                    eprintln!("Failed to send data: {:?}", result);
                }
            }
        });
        Self {
            sender,
            sink: Vec::new(),
        }
    }
}

// The crossterm backend writes to the terminal handle.
impl std::io::Write for TerminalHandle {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.sink.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let result = self.sender.send(self.sink.clone());
        if result.is_err() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                result.unwrap_err(),
            ));
        }

        self.sink.clear();
        Ok(())
    }
}

#[derive(Clone)]
struct AppServer {
    clients: Arc<Mutex<HashMap<usize, (SshTerminal, App, Channel<Msg>)>>>,
    id: usize,
    db: DatabaseConnection,
}

impl AppServer {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            id: 0,
            db: DB.clone(),
        }
    }

    pub async fn get_channel(&mut self, channel_id: usize) -> (SshTerminal, App, Channel<Msg>) {
        let mut clients = self.clients.lock().await;
        clients.remove(&channel_id).unwrap()
    }

    pub async fn run(&mut self) -> HorseResult<()> {
        let clients = self.clients.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

                for (_, (terminal, app, _)) in clients.lock().await.iter_mut() {
                    app.counter += 1;

                    terminal
                        .draw(|f| {
                            let area = f.area();
                            f.render_widget(Clear, area);
                            let style = match app.counter % 3 {
                                0 => Style::default().fg(Color::Red),
                                1 => Style::default().fg(Color::Green),
                                _ => Style::default().fg(Color::Blue),
                            };
                            let paragraph = Paragraph::new(format!("Counter: {}", app.counter))
                                .alignment(ratatui::layout::Alignment::Center)
                                .style(style);
                            let block = Block::default()
                                .title("Press 'c' to reset the counter!")
                                .borders(Borders::ALL);
                            f.render_widget(paragraph.block(block), area);
                        })
                        .unwrap();
                }
            }
        });

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

    // fn git_upload_pack(&self, repo_path: &str) -> HorseResult<String> {
    //     // 使用 git2 库执行 'git-upload-pack' 操作（拉取）
    //     let repo = Repository::open(repo_path).unwrap();
    //
    //     // 获取远程仓库
    //     let mut remote = repo.find_remote("origin").unwrap();
    //
    //     // 配置 fetch 选项
    //     let mut fetch_opts = FetchOptions::new();
    //     let mut remote_callbacks = git2::RemoteCallbacks::new();
    //     remote_callbacks.transfer_progress(|progress| {
    //         println!("Progress: {}%", progress.received_objects());
    //         true
    //     });
    //     fetch_opts.remote_callbacks(remote_callbacks);
    //
    //     // 执行 fetch 操作
    //     remote
    //         .fetch(&["refs/heads/main"], Some(&mut fetch_opts), None)
    //         .unwrap();
    //
    //     // 获取拉取后的信息并通过 channel 返回
    //     let head = repo.head().unwrap();
    //     let object = repo
    //         .find_object(head.target().unwrap(), Some(ObjectType::Commit))
    //         .unwrap();
    //     let commit = object.peel_to_commit().unwrap();
    //
    //     let commit_msg = format!(
    //         "Latest commit: {}",
    //         commit.message().unwrap_or("No message")
    //     );
    //
    //     Ok(commit_msg)
    // }
    //
    // fn git_receive_pack(&self, repo_path: &str) -> HorseResult<()> {
    //     // 使用 git2 库执行 'git-receive-pack' 操作（推送）
    //     let repo = Repository::open(repo_path).unwrap();
    //
    //     // 设置推送选项
    //     let mut push_opts = PushOptions::new();
    //     let mut remote = repo.find_remote("origin").unwrap();
    //
    //     // 执行推送操作（将本地更改推送到远程）
    //     remote
    //         .push(&["refs/heads/main:refs/heads/main"], Some(&mut push_opts))
    //         .unwrap();
    //
    //     Ok(())
    // }
}

impl Server for AppServer {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        let s = self.clone();
        self.id += 1;
        s
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
        let terminal_handle = TerminalHandle::start(session.handle(), channel.id()).await;

        let backend = CrosstermBackend::new(terminal_handle);

        // the correct viewport area will be set when the client request a pty
        let options = TerminalOptions {
            viewport: Viewport::Fixed(Rect::default()),
        };

        let terminal = Terminal::with_options(backend, options)?;
        let app = App::new();

        let mut clients = self.clients.lock().await;
        clients.insert(self.id, (terminal, app, channel));

        Ok(true)
    }

    /// Check authentication using the "password" method. Russh
    /// makes sure rejection happens in time
    /// `config.auth_rejection_time`, except if this method takes more
    /// than that.
    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
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

        let Some(sa) = SshPk::find_by_id((pk.algorithm().to_string(), data))
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
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
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
        use std::process::Stdio;
        let command = String::from_utf8_lossy(data);
        tracing::info!("EXEC: {}", command);
        let (_, _, channel) = self.get_channel(self.id).await;

        if command.starts_with("git-upload-pack") {
            // 处理 git-upload-pack 命令（通常是克隆）
            let output = tokio::process::Command::new("git")
                .arg("upload-pack")
                .arg("./repos/a")
                .stdout(Stdio::piped())
                .output()
                .await?;

            tracing::info!("{:?}", &output.stdout);
            channel.data(&output.stdout[..]).await?;
        } else if command.starts_with("git-receive-pack") {
            // 处理 git-receive-pack 命令（通常是推送）
            let output = tokio::process::Command::new("git")
                .arg("receive-pack")
                .arg("./repos/a")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .output()
                .await?;
            tracing::info!("{:?}", String::from_utf8_lossy(&output.stdout));
            if let Err(err) = channel.data(&output.stdout[..]).await {
                tracing::error!("Failed to send data: {:?}", err);
            }
        }

        // if command.starts_with("git-upload-pack") {
        //     // 处理 git-upload-pack 命令（通常是克隆）
        //     let commit_msg = self.git_upload_pack("./repos/a")?;
        //     channel.data(commit_msg.as_bytes()).await?;
        // } else if command.starts_with("git-receive-pack") {
        //     // 处理 git-receive-pack 命令（通常是推送）
        //     self.git_receive_pack("./repos/a")?;
        //     // 返回推送成功信息
        //     channel.data(&b"Push successful"[..]).await?;
        // }

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
        _signal: Sig,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> HorseResult<()> {
        match data {
            // Pressing 'q' closes the connection.
            b"q" => {
                self.clients.lock().await.remove(&self.id);
                session.close(channel)?;
            }
            // Pressing 'c' resets the counter for the app.
            // Only the client with the id sees the counter reset.
            b"c" => {
                let mut clients = self.clients.lock().await;
                let (_, app, _) = clients.get_mut(&self.id).unwrap();
                app.counter = 0;
            }
            _ => {}
        }

        Ok(())
    }

    /// The client's window size has changed.
    async fn window_change_request(
        &mut self,
        _: ChannelId,
        col_width: u32,
        row_height: u32,
        _: u32,
        _: u32,
        _: &mut Session,
    ) -> HorseResult<()> {
        let rect = Rect {
            x: 0,
            y: 0,
            width: col_width as u16,
            height: row_height as u16,
        };

        let mut clients = self.clients.lock().await;
        let (terminal, _, _) = clients.get_mut(&self.id).unwrap();
        terminal.resize(rect)?;

        Ok(())
    }

    /// The client requests a pseudo-terminal with the given
    /// specifications.
    ///
    /// **Note:** Success or failure should be communicated to the client by calling
    /// `session.channel_success(channel)` or `session.channel_failure(channel)` respectively.
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _: &str,
        col_width: u32,
        row_height: u32,
        _: u32,
        _: u32,
        _: &[(Pty, u32)],
        session: &mut Session,
    ) -> HorseResult<()> {
        let rect = Rect {
            x: 0,
            y: 0,
            width: col_width as u16,
            height: row_height as u16,
        };

        let mut clients = self.clients.lock().await;
        let (terminal, _, _) = clients.get_mut(&self.id).unwrap();
        terminal.resize(rect)?;

        session.channel_success(channel)?;

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

    /// Called when the client sends EOF to a channel.
    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("Channel Eof");
        Ok(())
    }
}

impl Drop for AppServer {
    fn drop(&mut self) {
        let id = self.id;
        let clients = self.clients.clone();
        tokio::spawn(async move {
            let mut clients = clients.lock().await;
            clients.remove(&id);
        });
    }
}

pub async fn run() -> HorseResult<()> {
    let mut server = AppServer::new();
    server.run().await.expect("Failed running server");
    Ok(())
}
