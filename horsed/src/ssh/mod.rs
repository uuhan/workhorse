use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Stdio;
use std::str::from_utf8;
use std::sync::Arc;

use crate::db::entity::prelude::{SshAuth, User};
use crate::git::repo::Repo;
use crate::key::KEY;
use crate::prelude::*;
use anyhow::Context;
use clean_path::Clean;
use colored::{Color, Colorize};
use russh::keys::ssh_key::{Certificate, PublicKey};
use russh::{server::*, MethodSet};
use russh::{Channel, ChannelId, Sig};
use sea_orm::{DatabaseConnection, EntityTrait, ModelTrait};
use shellwords::split;
use tokio::io::AsyncReadExt;
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
    /// 当前的环境变量
    env: HashMap<String, String>,
}

impl Clone for AppServer {
    fn clone(&self) -> Self {
        Self {
            clients: self.clients.clone(),
            db: DB.clone(),
            handle: None,
            action: String::new(),
            env: HashMap::new(),
        }
    }
}

impl AppServer {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            handle: None,
            db: DB.clone(),
            action: String::new(),
            env: HashMap::new(),
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

    /// 服务端 git 命令处理
    pub async fn git(&mut self, command: Vec<String>) -> HorseResult<()> {
        // git clone ssh://git@127.0.0.1:2222/repos/a
        // git-upload-pack '/repos/a'
        tracing::info!("GIT: {}", command.join(" "));
        let mut handle = self.handle.take().context("FIXME: NO HANDLE").unwrap();

        let git = &command.first().context("FIXME: GIT PUSH/CLONE")?;
        let repo = &command.get(1).context("FIXME: GIT PUSH/CLONE")?;

        let mut repo_path = PathBuf::from(repo);
        repo_path = repo_path
            .strip_prefix("/")
            .context("Repo strip_prefix")?
            .into();
        // 清理路径
        repo_path = repo_path.clean();

        // 如果提供的地址包含 .. 等路径，则拒绝请求
        if repo_path.components().next() == Some(std::path::Component::ParentDir) {}

        if let Some(fst) = repo_path.components().next() {
            if fst == std::path::Component::ParentDir {
                handle.finish().await?;
                return Ok(());
            }

            let parent = fst
                .as_os_str()
                .to_str()
                .context(format!("目录名非法: {:?}", repo_path))
                .unwrap();

            let path = std::env::current_dir()?;

            // 仓库存放在 repos 目录下
            if parent != "repos" {
                repo_path = path.join("repos").join(repo_path);
            } else {
                repo_path = path.join(repo_path);
            }

            repo_path = repo_path.clean();
        }

        // 仓库名称统一添加 .git 后缀
        if repo_path.extension() != Some(OsStr::new("git")) && !repo_path.set_extension("git") {
            tracing::error!("无效仓库路径: {:?}", repo_path);
            handle.finish().await?;
            return Ok(());
        }

        tracing::info!("GIT REPO: {}", repo_path.display());
        let mut repo = Repo::from(repo_path);

        match git.as_str() {
            // git clone
            "git-upload-pack" => {
                // TODO: 需要对仓库进行检查
                if !repo.exists() {
                    tracing::warn!("克隆仓库不存在: {:?}", repo.path().display());
                    handle.finish().await?;
                    return Ok(());
                }

                tokio::spawn(async move {
                    match handle
                        .exec(Command::new("git").arg("upload-pack").arg(repo.path()))
                        .await
                    {
                        Ok(mut cmd) => {
                            handle.exit(cmd.wait().await?).await?;
                            Result::<_, HorseError>::Ok(())
                        }
                        Err(err) => {
                            tracing::error!("git upload-pack failed: {}", err);
                            Ok(())
                        }
                    }
                });
            }
            // git push
            "git-receive-pack" => {
                // 如果仓库目录不存在
                if !repo.exists() {
                    handle.info("成功创建仓库, 接受第一次推送...").await?;
                    repo.init_bare().await?;
                }

                tokio::spawn(async move {
                    match handle
                        .exec(Command::new("git-receive-pack").arg(repo.path()))
                        .await
                    {
                        Ok(mut cmd) => {
                            handle.exit(cmd.wait().await?).await?;
                            Result::<_, HorseError>::Ok(())
                        }
                        Err(err) => {
                            tracing::error!("git receive-pack: {}", err);
                            Ok(())
                        }
                    }
                });
            }
            unkonwn => {
                tracing::error!("不支持的GIT命令: {unkonwn}");
                return Ok(());
            }
        }

        Ok(())
    }

    /// 服务端执行命令
    pub async fn cmd(&mut self, command: Vec<String>) -> HorseResult<()> {
        tracing::info!("CMD: {}", command.join(" "));
        let mut handle = self
            .handle
            .take()
            .context("FIXME: NO HANDLE".color(Color::Red))?;
        tokio::spawn(async move {
            #[cfg(windows)]
            match handle
                .exec(Command::new("cmd.exe").arg("/C").args(command))
                .await
            {
                Ok(mut cmd) => {
                    handle.exit(cmd.wait().await?).await?;
                    Result::<_, HorseError>::Ok(())
                }
                Err(err) => {
                    tracing::error!("command failed: {}", err);
                    Ok(())
                }
            }
            #[cfg(not(windows))]
            match handle
                .exec(Command::new("sh").arg("-c").arg(command.join(" ")))
                .await
            {
                Ok(mut cmd) => {
                    handle.exit(cmd.wait().await?).await?;
                    Result::<_, HorseError>::Ok(())
                }
                Err(err) => {
                    tracing::error!("command failed: {}", err);
                    Ok(())
                }
            }
        });

        Ok(())
    }

    /// ### 服务端 just 指令
    ///
    /// 用于持续集成的自动化任务, 往 just@xxx.xxx.xxx.xxx push 代码即可触发构建
    /// 目前主要用于跟 git 工作流配合
    ///
    /// FIXME: git push 会主动断开
    pub async fn just(&mut self, command: Vec<String>) -> HorseResult<()> {
        // git push ssh://just@127.0.0.1:2222/repo-name
        // git-upload-pack '/repo-name'
        tracing::info!("GIT: {}", command.join(" "));
        let env_git = &command.first().context("FIXME: GIT ARGS")?;
        let env_repo = &command.get(1).context("FIXME: GIT ARGS")?;

        let mut repo_path = PathBuf::from(env_repo);
        repo_path = repo_path
            .strip_prefix("/")
            .context("REPO STRIP_PREFIX")?
            .into();
        // 清理路径
        repo_path = repo_path.clean();
        let repo_path_origin = repo_path.clone();

        let mut handle = self.handle.take().context("FIXME: NO HANDLE")?;

        if let Some(fst) = repo_path.components().next() {
            // 如果提供的地址包含 .. 等路径，则拒绝请求
            if fst == std::path::Component::ParentDir {
                tracing::warn!("拒绝仓库请求, 路径不合法: {}", repo_path.display());
                handle.finish().await?;
                return Ok(());
            }

            let parent = fst
                .as_os_str()
                .to_str()
                .context(format!("目录名非法: {:?}", repo_path))?;

            let current_dir = std::env::current_dir()?;

            // 仓库存放在 repos 目录下
            if parent != "repos" {
                repo_path = current_dir.join("repos").join(repo_path);
            } else {
                repo_path = current_dir.join(repo_path);
            }

            repo_path = repo_path.clean();
        }

        // 裸仓库名称统一添加 .git 后缀
        if repo_path.extension() != Some(OsStr::new("git")) && !repo_path.set_extension("git") {
            tracing::error!("无效仓库路径: {:?}", repo_path);
            handle.finish().await?;
            return Ok(());
        }

        let mut repo = Repo::from(&repo_path);
        tracing::info!("GIT REPO: {}", repo.path().display());

        match env_git.as_str() {
            // 响应 git clone/pull/fetch 请求
            // just 命令在拉取的时候单纯返回 pack
            "git-upload-pack" => {
                // TODO: 需要对仓库进行检查
                if !repo.exists() {
                    // TODO: 通知客户端失败原因
                    tracing::warn!("克隆仓库不存在: {}", repo.path().display());
                    handle.finish().await?;
                    return Ok(());
                }

                tokio::spawn(async move {
                    match handle
                        .exec(Command::new("git").arg("upload-pack").arg(repo.path()))
                        .await
                    {
                        Ok(mut cmd) => {
                            handle.exit(cmd.wait().await?).await?;
                            Result::<_, HorseError>::Ok(())
                        }
                        Err(err) => {
                            tracing::error!("git upload-pack failed: {}", err);
                            Ok(())
                        }
                    }
                });
            }

            // 响应 git push 请求
            // just 命令此时会
            // 1. 收集 pack 入库
            // 2. 检出代码用于构建
            // 3. 执行项目的 just 命令, 项目必须包含 justfile 文件
            "git-receive-pack" => {
                // 如果仓库目录不存在
                if !repo.exists() {
                    repo.init_bare().await?;
                }

                tokio::spawn(async move {
                    match handle
                        .exec(Command::new("git").arg("receive-pack").arg(repo.path()))
                        .await
                    {
                        Ok(mut cmd) => {
                            // 收集 pack 入库
                            cmd.wait().await?;
                            handle.info("代码推送成功, 开始构建...").await?;

                            let work_path = std::env::current_dir()?
                                .join("workspace")
                                .join(repo_path_origin);
                            if !work_path.exists() {
                                tracing::info!("CREATE DIR: {}", work_path.display());
                                std::fs::create_dir_all(&work_path).context("创建工作目录失败")?;
                            }

                            // 编译目录
                            handle.info("检出代码到工作目录...").await?;
                            repo.checkout(&work_path, Some("HEAD"))
                                .await
                                .context("检出代码失败")?;

                            handle.info("开始构建...").await?;
                            let mut cmd = Command::new("just");
                            cmd.current_dir(&work_path);
                            cmd.arg("-f");
                            cmd.arg(work_path.join("justfile"));
                            cmd.arg("build");

                            cmd.stdout(Stdio::piped());
                            cmd.stderr(Stdio::piped());

                            // TODO: 需要有更好的方式处理命令调用
                            let mut cmd = cmd.spawn()?;
                            handle.info("执行命令...").await?;

                            let mut stdout = cmd.stdout.take().unwrap();
                            let mut stderr = cmd.stderr.take().unwrap();

                            let fut = async move {
                                const BUF_SIZE: usize = 1024 * 32;
                                let mut out_buf = [0u8; BUF_SIZE];
                                loop {
                                    let read = stdout.read(&mut out_buf).await?;
                                    if read == 0 {
                                        break;
                                    }
                                    handle.log_raw(&out_buf[..read]).await?;
                                }

                                loop {
                                    let read = stderr.read(&mut out_buf).await?;
                                    if read == 0 {
                                        break;
                                    }
                                    handle.log_raw(&out_buf[..read]).await?;
                                }

                                handle.info("🎉 构建完成").await?;
                                handle.exit(cmd.wait().await?).await?;

                                Ok::<(), HorseError>(())
                            };

                            tokio::spawn(fut);
                            Ok(())
                        }
                        Err(err) => {
                            tracing::error!("git receive-pack failed: {}", err);
                            Result::<_, HorseError>::Ok(())
                        }
                    }
                });
            }
            unkonwn => {
                tracing::error!("不支持的GIT命令: {unkonwn}");
                return Ok(());
            }
        }

        Ok(())
    }

    /// ## 服务端构建
    ///
    /// 1. 从 repos 目录下 clone 仓库
    /// 2. clone 仓库到 workspace 目录下
    /// 3. 执行 cargo build
    ///
    /// ### 需要环境变量
    ///
    /// - REPO: 仓库名称
    /// - BRANCH: 分支名称
    ///
    /// ### 示例
    ///
    /// ```bash
    /// ssh -o SetEnv="REPO=workhorse BRANCH=main" build@xxx.xxx.xxx.xxx -- -p horsed
    /// ```
    pub async fn build(&mut self, command: Vec<String>) -> HorseResult<()> {
        let env_repo = self.env.get("REPO").context("REPO 环境变量未设置")?;
        let env_branch = self.env.get("BRANCH").context("BRANCH 环境变量未设置")?;

        tracing::info!("BUILD: {}", command.join(" "));
        let mut repo_path = std::env::current_dir()?.join("repos").join(env_repo);
        repo_path.set_extension("git");
        repo_path = repo_path.clean();

        let repo = Repo::from(repo_path);

        if !repo.exists() {
            tracing::error!("仓库不存在: {}", repo.path().display());
        }

        let mut work_path = std::env::current_dir()?.join("workspace").join(env_repo);
        work_path = work_path.clean();
        if !work_path.exists() {
            std::fs::create_dir_all(&work_path).context("创建工作目录失败")?;
        }

        // 编译目录
        repo.checkout(&work_path, Some(env_branch)).await?;
        // let work_repo = Repo::clone(repo.path(), work_path, Some(env_branch))
        //     .await
        //     .context("克隆仓库失败")?;

        let mut cmd = Command::new("cargo");
        cmd.current_dir(&work_path);
        cmd.arg("build");
        cmd.arg("--color=always");
        cmd.arg("--manifest-path");
        cmd.arg(work_path.join("Cargo.toml"));
        cmd.args(command);

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let handle = self.handle.take().context("FIXME: NO HANDLE").unwrap();

        tokio::spawn(async move {
            // if let Err(err) = handle.exec(&mut cmd).await {
            //     tracing::error!("Exec Error: {}", err);
            // }

            // Run the command
            let mut cmd = cmd.spawn()?;

            let mut stdout = cmd.stdout.take().unwrap();
            let mut stderr = cmd.stderr.take().unwrap();

            let mut o_output = handle.make_writer();
            let mut e_output = handle.make_writer();

            let mut o_ready = false;
            let mut e_ready = false;
            loop {
                tokio::select! {
                    o = tokio::io::copy(&mut stdout, &mut o_output), if !o_ready => {
                        match o {
                            Ok(len) => {
                                tracing::debug!("send data: {}", len);
                                if len == 0 {
                                    o_ready = true;
                                }
                            },
                            Err(e) => {
                                tracing::error!("send data error: {}", e);
                                break;
                            }
                        }
                    },
                    e = tokio::io::copy(&mut stderr, &mut e_output), if !e_ready => {
                        match e {
                            Ok(len) => {
                                tracing::debug!("send stderr data: {}", len);
                                if len == 0 {
                                    e_ready = true;
                                }
                            },
                            Err(e) => {
                                tracing::error!("send stderr data error: {}", e);
                                break;
                            }
                        }
                    },
                    else => {
                        break;
                    }
                }
            }

            handle.exit(cmd.wait().await?).await?;
            Result::<_, HorseError>::Ok(())
        });

        Ok(())
    }
}

impl Server for AppServer {
    type Handler = Self;
    /// 创建新连接
    fn new_client(&mut self, peer: Option<std::net::SocketAddr>) -> Self {
        tracing::info!("新建连接: {:?}", peer);
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
        self.env
            .insert(key.to_uppercase().to_string(), value.to_string());
        // session.channel_success(channel)?;
        Ok(())
    }

    /// The client requests a shell.
    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("ssh shell request");
        // session.channel_success(channel)?;
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
        let command = from_utf8(data).context(format!("无效请求: {:?}", &data))?;
        let command = split(command).context(format!("无效命令: {command}"))?;

        match self.action.as_str() {
            "git" => self.git(command).await?,
            "cmd" => self.cmd(command).await?,
            "just" => self.just(command).await?,
            "build" => self.build(command).await?,
            other => {
                tracing::warn!("未知命令: {other}");
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
