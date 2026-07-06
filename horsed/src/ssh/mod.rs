#![allow(unused_imports, unused_variables, dead_code)]
use std::collections::{HashMap, VecDeque};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::str::from_utf8;
use std::sync::Arc;

use crate::db::entity::prelude::{SshPk, User};
use crate::db::entity::{ssh_pk, user};
use crate::git::repo::Repo;
use crate::prelude::*;
use anyhow::{anyhow, Context};
use clean_path::Clean;
use colored::{Color, Colorize};
use flate2::write::ZlibEncoder;
use flate2::Compression;
#[cfg(not(windows))]
use pty_process::{Pts, Pty, Size};
use russh::keys::{Certificate, PublicKey};
use russh::{server::*, MethodSet};
use russh::{Channel, ChannelId, Sig};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, DbConn, EntityTrait,
    ModelTrait, PaginatorTrait, QueryFilter, QueryOrder,
};
use shellwords::split;
use stable::buffer;
use stable::{
    data::{
        v2::{self, *},
        *,
    },
    task::TaskManager,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpSocket, ToSocketAddrs};
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::Instrument;

mod handle;
pub mod health;
mod jobs;
pub mod setup;
use handle::ChannelHandle;
use jobs::{JobEvent, JobRecord, JobRegistry};
use v2::Body;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
struct SessionUser {
    id: i32,
    name: String,
    role: String,
}

impl SessionUser {
    fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

async fn copy_with_job<R, W>(
    reader: &mut R,
    writer: &mut W,
    job: Arc<JobRecord>,
) -> HorseResult<u64>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut total = 0_u64;
    let mut buf = [0_u8; 8192];
    loop {
        let len = reader.read(&mut buf).await?;
        if len == 0 {
            break;
        }
        writer.write_all(&buf[..len]).await?;
        job.append_output(&buf[..len]).await;
        total = total.saturating_add(len as u64);
    }
    Ok(total)
}

fn cmd_shell_arg(shell: &str) -> &'static str {
    let shell_name = Path::new(shell)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(shell);

    if matches!(shell_name, "bash" | "zsh") {
        "-ic"
    } else {
        "-c"
    }
}

#[derive(serde::Serialize)]
struct AdminUserRow {
    id: i32,
    name: String,
    nick: Option<String>,
    email: Option<String>,
    role: String,
    enabled: bool,
    key_count: u64,
}

#[derive(serde::Serialize)]
struct AdminKeyRow {
    alg: String,
    key: String,
    user_id: i32,
    user_name: Option<String>,
    enabled: bool,
    comment: Option<String>,
}

pub struct AppServer {
    /// 客户端连接
    id: usize,
    #[cfg(not(windows))]
    /// 一些共享数据
    clients: Arc<Mutex<HashMap<usize, (Pty, Pts)>>>,
    #[cfg(windows)]
    /// 一些共享数据
    clients: Arc<Mutex<HashMap<usize, winptyrs::PTY>>>,
    /// 任务管理器
    tm: TaskManager,
    /// 数据库连接
    db: DatabaseConnection,
    /// 当前 Client 的 ChannelHandle
    handle: Option<ChannelHandle>,
    /// 当前 Client 请求的 action 名称（如 cargo/get/put/cmd）
    action: String,
    /// 当前认证通过的账户信息
    user: Option<SessionUser>,
    /// 当前的环境变量
    env: HashMap<String, String>,
    /// 任务输出缓存与 attach 管理
    jobs: JobRegistry,
}

impl Clone for AppServer {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            clients: self.clients.clone(),
            tm: TaskManager::default(),
            db: self.db.clone(),
            handle: None,
            action: String::new(),
            user: None,
            env: HashMap::new(),
            jobs: self.jobs.clone(),
        }
    }
}

impl AppServer {
    pub fn new(db: DbConn) -> Self {
        Self {
            id: 0,
            clients: Arc::new(Mutex::new(HashMap::new())),
            tm: TaskManager::default(),
            handle: None,
            db,
            action: String::new(),
            user: None,
            env: HashMap::new(),
            jobs: JobRegistry::default(),
        }
    }

    pub async fn run<A: ToSocketAddrs + Send>(
        &mut self,
        config: Config,
        addrs: A,
    ) -> HorseResult<()> {
        self.run_on_address(Arc::new(config), addrs).await?;
        Ok(())
    }

    fn trace_id(&self) -> &str {
        self.env
            .get("HORSE_TRACE_ID")
            .map(String::as_str)
            .unwrap_or("")
    }

    fn debug_enabled(&self) -> bool {
        !self.trace_id().is_empty()
    }

    fn user_name(&self) -> &str {
        self.user
            .as_ref()
            .map(|user| user.name.as_str())
            .unwrap_or("")
    }

    fn require_admin(&self) -> HorseResult<()> {
        if self.user.as_ref().is_some_and(SessionUser::is_admin) {
            return Ok(());
        }

        Err(anyhow!("需要管理员权限").into())
    }

    /// 服务端 git 命令处理
    #[tracing::instrument(skip(self), err)]
    pub async fn git(&mut self, command: Vec<String>) -> HorseResult<()> {
        // git clone ssh://git@127.0.0.1:2222/repos/a
        // git-upload-pack '/repos/a'
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
        if let Some(fst) = repo_path.components().next() {
            if fst == std::path::Component::ParentDir {
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
            return Ok(());
        }

        tracing::info!("GIT REPO: {}", repo_path.display());
        let mut repo = Repo::from(repo_path);
        let task = self.tm.spawn_handle();

        match git.as_str() {
            // git clone
            "git-upload-pack" => {
                // TODO: 需要对仓库进行检查
                if !repo.exists() {
                    tracing::warn!("克隆仓库不存在: {:?}", repo.path().display());
                    return Ok(());
                }

                task.spawn(async move {
                    match handle
                        .exec_io(Command::new("git").arg("upload-pack").arg(repo.path()))
                        .await
                    {
                        Ok(mut cmd) => {
                            handle.exit(cmd.wait().await?).await?;
                        }
                        Err(err) => {
                            tracing::error!("git upload-pack failed: {}", err);
                        }
                    }
                    Ok(())
                });
            }
            // git push
            "git-receive-pack" => {
                // 如果仓库目录不存在
                if !repo.exists() {
                    handle.info("成功创建仓库, 接受第一次推送...").await?;
                    repo.init_bare().await?;
                }

                task.spawn(async move {
                    match handle
                        .exec_io(Command::new("git").arg("receive-pack").arg(repo.path()))
                        .await
                    {
                        Ok(mut cmd) => {
                            handle.exit(cmd.wait().await?).await?;
                        }
                        Err(err) => {
                            tracing::error!("git receive-pack: {}", err);
                        }
                    }
                    Ok(())
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
    #[tracing::instrument(skip(self), err)]
    pub async fn cmd(&mut self, command: Vec<String>) -> HorseResult<()> {
        tracing::info!("CMD: {}", command.join(" "));

        let env_repo = self.env.get("REPO").cloned();
        let env_branch = self.env.get("BRANCH").cloned();

        let shell = if let Some(shell) = self.env.get("SHELL").cloned() {
            shell
        } else if cfg!(windows) {
            "powershell.exe".to_string()
        } else {
            "bash".to_string()
        };

        // 如果命令中包含 REPO 或者 BRANCH 环境变量, 则切换到工作目录执行命令
        let cmd_dir = if let (Some(env_repo), Some(_)) = (env_repo, env_branch) {
            let mut repo_path = PathBuf::from(env_repo);
            // 去除开头的 /
            if let Ok(stripped) = repo_path.strip_prefix("/") {
                repo_path = stripped.to_path_buf();
            }

            // 清理路径
            repo_path = repo_path.clean();

            // 裸仓库名称统一添加 .git 后缀
            if repo_path.extension() != Some(OsStr::new("git")) && !repo_path.set_extension("git") {
                tracing::error!("无效仓库路径: {:?}", repo_path);
                let handle = self.handle.take().context("FIXME: NO HANDLE")?;
                handle
                    .fail_with_error(
                        2,
                        "HSSH_REPO_PATH_INVALID",
                        format!("无效仓库路径: {:?}", repo_path),
                    )
                    .await?;
                return Ok(());
            }

            let mut work_path = std::env::current_dir()?.join("workspace").join(repo_path);
            // 构建目录不包含 .git 后缀
            work_path.set_extension("");

            if work_path.exists() {
                work_path
            } else {
                std::env::current_dir()?
            }
        } else {
            std::env::current_dir()?
        };

        let handle = self
            .handle
            .take()
            .context("FIXME: NO HANDLE".color(Color::Red))?;
        let command_line = command.join(" ");
        let owner = self.user_name().to_string();
        let job = self
            .jobs
            .create_job(owner, "cmd", command_line.clone())
            .await;
        handle.info(format!("job_id={}", job.id())).await?;
        let task = self.tm.spawn_handle();
        let span = tracing::info_span!("spawn", command = ?command, cmd_dir = ?cmd_dir);
        let mut cmd = Command::new(&shell);
        cmd.envs(&self.env);

        task.spawn(
            async move {
                let mut final_code = 1_i32;
                let result: anyhow::Result<()> = async {
                    #[cfg(target_os = "windows")]
                    {
                        #[allow(unused_imports)]
                        use std::os::windows::process::CommandExt;
                        const CREATE_NO_WINDOW: u32 = 0x08000000;

                        cmd.creation_flags(CREATE_NO_WINDOW);
                    }

                    cmd.stdout(Stdio::piped());
                    cmd.stderr(Stdio::piped());

                    // Use interactive mode for bash/zsh so their rc files
                    // (`.bashrc` / `.zshrc`) are loaded by default. Many
                    // developer toolchains put PATH setup there.
                    let shell_arg = cmd_shell_arg(&shell);
                    cmd.current_dir(&cmd_dir)
                        .kill_on_drop(true)
                        .arg(shell_arg)
                        .arg(command.join(" "));

                    let mut cmd = match cmd.spawn() {
                        Ok(cmd) => cmd,
                        Err(err) => {
                            final_code = 127;
                            tracing::error!("spawn: `{}` failed: {}", shell, err);
                            handle
                                .fail_with_error(
                                    127,
                                    "HSSH_SHELL_SPAWN_FAILED",
                                    format!("spawn: `{}` failed: {}", shell, err),
                                )
                                .await?;
                            return Ok(());
                        }
                    };

                    let mut stdout = cmd.stdout.take().unwrap();
                    let mut stderr = cmd.stderr.take().unwrap();

                    let mut cout = handle.make_writer();
                    let mut eout = handle.make_writer();
                    let out_job = job.clone();
                    let err_job = job.clone();

                    let cout_fut = copy_with_job(&mut stdout, &mut cout, out_job);
                    let eout_fut = copy_with_job(&mut stderr, &mut eout, err_job);

                    let (c1, c2) = futures::future::try_join(cout_fut, eout_fut).await?;
                    tracing::debug!("write: stdout={}, stderr={}", c1, c2);

                    let status = cmd.wait().await?;
                    final_code = status.code().unwrap_or(128);
                    if status.success() {
                        tracing::info!("成功");
                    } else {
                        tracing::warn!("失败: {}", status);
                    }

                    handle.exit(status).await?;
                    Ok(())
                }
                .await;

                job.finish(final_code).await;
                result
            }
            .instrument(span),
        );

        Ok(())
    }

    /// 获取服务端文件
    #[tracing::instrument(skip(self), err)]
    pub async fn get(&mut self, files: Vec<String>) -> HorseResult<()> {
        tracing::info!("GET: {}", files.join(" "));

        let env_repo = self.env.get("REPO").context("REPO 环境变量未设置")?;
        let _env_branch = self.env.get("BRANCH").context("BRANCH 环境变量未设置")?;

        let mut repo_path = PathBuf::from(env_repo);
        // 去除开头的 /
        if let Ok(stripped) = repo_path.strip_prefix("/") {
            repo_path = stripped.to_path_buf();
        }

        // 清理路径
        repo_path = repo_path.clean();

        // 裸仓库名称统一添加 .git 后缀
        if repo_path.extension() != Some(OsStr::new("git")) && !repo_path.set_extension("git") {
            tracing::error!("无效仓库路径: {:?}", repo_path);
            let handle = self.handle.take().context("FIXME: NO HANDLE")?;
            handle
                .error(format!("无效仓库路径: {:?}", repo_path))
                .await?;
            handle.eof().await?;
            handle.close().await?;
            return Ok(());
        }

        let mut work_path = std::env::current_dir()?.join("workspace").join(repo_path);
        // 构建目录不包含 .git 后缀
        work_path.set_extension("");

        let handle = self
            .handle
            .take()
            .context("FIXME: NO HANDLE".color(Color::Red))?;
        let task = self.tm.spawn_handle();

        let file = files.first().context("FIXME: NO FILE")?;
        let file_path = PathBuf::from(file);
        if let Some(fst) = file_path.components().next() {
            if fst == std::path::Component::ParentDir {
                tracing::warn!("拒绝文件请求, 只能拷贝工作目录文件: {}", file);
                handle
                    .error(format!("拒绝文件请求, 路径不合法: {}", file))
                    .await?;
                handle.eof().await?;
                handle.close().await?;
                return Ok(());
            }
        }

        let t1 = task.clone();
        task.spawn(async move {
            let file_path = work_path.join(file_path).clean();

            if !file_path.exists() {
                handle
                    .error(format!("文件不存在: {}", file_path.display()))
                    .await?;
                handle.eof().await?;
                handle.close().await?;
                return Ok(());
            }

            let md = std::fs::metadata(&file_path)?;

            // 请求目录
            if md.is_dir() {
                // 1MB 的缓冲区
                #[allow(clippy::identity_op)]
                const BUF_SIZE: usize = 1024 * 1024 * 1;
                let (writer, mut reader) = buffer::new(BUF_SIZE);

                let tar_writer = ZlibEncoder::new(writer, Compression::default());
                let mut cout = handle.make_writer();

                // TODO: 目录无法提前知道大小
                let body = Body::GetFile(GetFile {
                    path: file_path.clone(),
                    size: None,
                    kind: GetKind::Directory,
                });
                let body = bincode::serialize(&body)?;
                let head = v2::head(body.len() as _);
                // HEADER:BODY(GetFile):FILE
                cout.write_all(head.as_bytes()).await?;
                cout.write_all(&body).await?;

                t1.spawn_blocking(async move {
                    let mut tardir = tar::Builder::new(tar_writer);
                    let path = file_path.file_name().unwrap();
                    // 同步阻塞
                    tardir.append_dir_all(path, &file_path)?;
                    let tar = tardir.into_inner()?;
                    let size_in = tar.total_in();
                    let size_out = tar.total_out();

                    tracing::info!("目录路径: {}", file_path.display());
                    tracing::info!(
                        "目录大小: {}/{} = {:.2}%",
                        size_in,
                        size_out,
                        size_out as f64 / size_in as f64 * 100.0
                    );

                    tar.finish()?;

                    Ok(())
                });

                let mut buf = vec![0; BUF_SIZE];
                // TODO: 使用异步 IO 读取缓冲区
                use std::io::Read;
                while let Ok(len) = reader.read(&mut buf) {
                    if len == 0 {
                        break;
                    }

                    cout.write_all(&buf[..len]).await?;
                }

                tracing::info!("目录传输完成!");
                cout.shutdown().await?;
                handle.eof().await?;
                return Ok(());
            }

            // 请求文件
            if md.is_file() {
                // 5MB 的缓冲区
                const BUF_SIZE: usize = 1024 * 1024;
                let (writer, mut reader) = buffer::new(BUF_SIZE);
                let mut tar_writer = ZlibEncoder::new(writer, Compression::default());

                let mut cout = handle.make_writer();
                let size = md.len();
                let body = Body::GetFile(GetFile {
                    path: file_path.clone(),
                    size: Some(size),
                    kind: GetKind::File,
                });
                let body = bincode::serialize(&body)?;
                let head = v2::head(body.len() as _);
                cout.write_all(head.as_bytes()).await?;
                cout.write_all(&body).await?;

                t1.spawn_blocking(async move {
                    let mut file = std::fs::File::open(&file_path)?;
                    while let Ok(len) = std::io::copy(&mut file, &mut tar_writer) {
                        if len == 0 {
                            break;
                        }
                    }

                    Ok(())
                });

                use std::io::Read;
                let mut buf = vec![0; BUF_SIZE];
                while let Ok(len) = reader.read(&mut buf) {
                    if len == 0 {
                        break;
                    }

                    cout.write_all(&buf[..len]).await?;
                }

                tracing::info!("文件传输完成!");
                cout.shutdown().await?;
                handle.eof().await?;
                return Ok(());
            }

            Ok(())
        });

        Ok(())
    }

    /// 类似 scp 命令, 拷贝服务器文件到本地
    pub async fn scp(&mut self, files: Vec<String>) -> HorseResult<()> {
        tracing::info!("GET: {}", files.join(" "));

        let env_repo = self.env.get("REPO").context("REPO 环境变量未设置")?;
        let _env_branch = self.env.get("BRANCH").context("BRANCH 环境变量未设置")?;

        let mut repo_path = PathBuf::from(env_repo);
        // 去除开头的 /
        if let Ok(stripped) = repo_path.strip_prefix("/") {
            repo_path = stripped.to_path_buf();
        }

        // 清理路径
        repo_path = repo_path.clean();

        // 裸仓库名称统一添加 .git 后缀
        if repo_path.extension() != Some(OsStr::new("git")) && !repo_path.set_extension("git") {
            tracing::error!("无效仓库路径: {:?}", repo_path);
            let handle = self.handle.take().context("FIXME: NO HANDLE")?;
            handle
                .error(format!("无效仓库路径: {:?}", repo_path))
                .await?;
            handle.eof().await?;
            handle.close().await?;
            return Ok(());
        }

        let mut work_path = std::env::current_dir()?.join("workspace").join(repo_path);
        // 构建目录不包含 .git 后缀
        work_path.set_extension("");

        let handle = self
            .handle
            .take()
            .context("FIXME: NO HANDLE".color(Color::Red))?;
        let task = self.tm.spawn_handle();

        let file = files.first().context("FIXME: NO FILE")?;
        let file_path = PathBuf::from(file);
        if let Some(fst) = file_path.components().next() {
            if fst == std::path::Component::ParentDir {
                tracing::warn!("拒绝文件请求, 只能拷贝工作目录文件: {}", file);
                handle
                    .error(format!("拒绝文件请求, 路径不合法: {}", file))
                    .await?;
                handle.eof().await?;
                handle.close().await?;
                return Ok(());
            }
        }

        let file_path = work_path.join(file_path);

        if !file_path.exists() {
            handle.error(format!("文件不存在: {}", file)).await?;
            handle.eof().await?;
            handle.close().await?;
            return Ok(());
        }

        task.spawn(async move {
            // TODO: 获取文件
            let mut file = tokio::fs::File::open(&file_path).await?;
            let mut cout = handle.make_writer();

            while let Ok(len) = tokio::io::copy(&mut file, &mut cout).await {
                if len == 0 {
                    break;
                }
            }

            cout.shutdown().await?;
            handle.eof().await?;

            Ok(())
        });

        Ok(())
    }

    /// 执行 ssh 命令
    #[tracing::instrument(skip(self))]
    #[allow(unused_mut, unreachable_code)]
    pub async fn ssh(&mut self, commands: Vec<String>) -> HorseResult<()> {
        tracing::info!("SSH: {}", commands.join(" "));
        let mut commands = VecDeque::from(commands);

        let env_repo = self.env.get("REPO").context("REPO 环境变量未设置")?;
        let _env_branch = self.env.get("BRANCH").context("BRANCH 环境变量未设置")?;

        let mut repo_path = PathBuf::from(env_repo);
        // 去除开头的 /
        if let Ok(stripped) = repo_path.strip_prefix("/") {
            repo_path = stripped.to_path_buf();
        }

        // 清理路径
        repo_path = repo_path.clean();

        // 裸仓库名称统一添加 .git 后缀
        if repo_path.extension() != Some(OsStr::new("git")) && !repo_path.set_extension("git") {
            tracing::error!("无效仓库路径: {:?}", repo_path);
            let handle = self.handle.take().context("FIXME: NO HANDLE")?;
            handle
                .error(format!("无效仓库路径: {:?}", repo_path))
                .await?;
            handle.eof().await?;
            handle.close().await?;
            return Ok(());
        }

        let mut work_path = std::env::current_dir()?.join("workspace").join(repo_path);
        // 构建目录不包含 .git 后缀
        work_path.set_extension("");

        #[allow(unused_mut)]
        let mut handle = self
            .handle
            .take()
            .context("FIXME: NO HANDLE".color(Color::Red))?;

        #[cfg(windows)]
        let shell = commands.pop_front().unwrap_or("powershell.exe".to_string());
        #[cfg(not(windows))]
        let shell = commands.pop_front().unwrap_or("bash".to_string());

        let ssh_span = tracing::info_span!("ssh", shell, commands = ?commands);
        let env = self.env.clone();
        let id = self.id;

        let task = self.tm.spawn_handle();
        let clients = self.clients.clone();

        #[cfg(not(windows))]
        task.spawn(
            async move {
                let mut cmd = pty_process::Command::new(&shell);
                cmd = cmd.envs(&env);
                cmd = cmd
                    .kill_on_drop(true)
                    .current_dir(&work_path)
                    .args(&commands);

                let mut clients = clients.lock().await;
                // TODO: resize at runtime?
                let Some((pty, pts)) = clients.remove(&id) else {
                    tracing::error!("empty pty?: {}", id);
                    handle.eof().await?;
                    handle.close().await?;
                    return Ok(());
                };
                drop(clients);

                let mut cmd = match cmd.spawn(pts) {
                    Ok(cmd) => cmd,
                    Err(err) => {
                        tracing::error!("spawn: {:?}", err);
                        handle
                            .fail_with_error(
                                127,
                                "HSSH_PTY_SPAWN_FAILED",
                                format!("spawn: `{}` failed: {}", shell, err),
                            )
                            .await?;
                        return Ok(());
                    }
                };

                let (mut stdout, mut stdin) = pty.into_split();

                let mut ch_writer = handle.make_writer();
                let mut ch_reader = handle.make_reader();

                let reader_fut = tokio::io::copy(&mut ch_reader, &mut stdin);
                let writer_fut = tokio::io::copy(&mut stdout, &mut ch_writer);

                tokio::select! {
                    io = futures::future::try_join(reader_fut, writer_fut) => {
                        drop(ch_reader);
                        if let Err(err) = io {
                            tracing::error!("io error: {:?}", err);
                            handle
                                .fail_with_error(1, "HSSH_PTY_IO_FAILED", format!("io error: {:?}", err))
                                .await?;
                        } else {
                            handle.exit(cmd.wait().await?).await?;
                        }
                    }
                    code = cmd.wait() => {
                        drop(ch_reader);
                        match code {
                            Ok(code) => {
                                tracing::info!("cmd exit with code: {code}");
                                handle.exit(code).await?;
                            }
                            Err(err) => {
                                tracing::error!("cmd error: {:?}", err);
                                handle
                                    .fail_with_error(
                                        1,
                                        "HSSH_PTY_WAIT_FAILED",
                                        format!("cmd error: {:?}", err),
                                    )
                                    .await?;
                            }
                        }
                    }
                }

                Ok(())
            }
            .instrument(ssh_span),
        );

        #[cfg(windows)]
        let task_block = task.clone();
        #[cfg(windows)]
        task.spawn(
            async move {
                use std::ffi::OsString;
                let appname = OsString::from(&shell);
                let work_path = OsString::from(&work_path);
                let args = OsString::from(commands.make_contiguous().join(" "));
                tracing::info!("windows pty");

                let mut clients = clients.lock().await;
                // TODO: resize at runtime?
                let Some(pty) = clients.get(&id) else {
                    tracing::error!("empty pty?: {}", id);
                    handle.eof().await?;
                    handle.close().await?;
                    return Ok(());
                };
                let pty = pty.clone();
                drop(clients);

                let pty1 = pty.clone();
                match pty.spawn(appname, Some(args), Some(work_path), None) {
                    Ok(_) => {
                        tracing::info!("conpty spawned");
                        let mut ch_writer = handle.make_writer();
                        let mut ch_reader = handle.make_reader();

                        task_block.spawn_blocking(async move {
                            let mut idle_count = 0u32;
                            loop {
                                match pty1.read(1024 * 4, false) {
                                    Ok(buf) if buf.is_empty() => {
                                        idle_count = idle_count.saturating_add(1);
                                        // 渐进退避: 前几次快速重试, 之后逐渐放慢
                                        let wait = match idle_count {
                                            1..=3 => tokio::task::yield_now().await,
                                            4..=20 => {
                                                tokio::time::sleep(
                                                    std::time::Duration::from_micros(100),
                                                )
                                                .await
                                            }
                                            _ => {
                                                tokio::time::sleep(
                                                    std::time::Duration::from_millis(1),
                                                )
                                                .await
                                            }
                                        };
                                    }
                                    Ok(buf) => {
                                        idle_count = 0;
                                        let buf = buf.as_encoded_bytes();
                                        ch_writer.write_all(&buf).await?;
                                        ch_writer.flush().await?;
                                    }
                                    Err(_) => break, // EOF or error
                                }
                            }
                            Ok(())
                        });

                        let mut buf = [0u8; 1024];
                        while let Ok(len) = ch_reader.read(&mut buf).await {
                            if len == 0 {
                                break;
                            }
                            let buf = &buf[..len];
                            let s = String::from_utf8_lossy(buf);
                            let os_str = OsStr::new(s.as_ref());
                            if let Ok(true) = pty.is_alive() {
                                match pty.write(os_str.to_owned()) {
                                    Ok(_) => {}
                                    Err(err) => {
                                        tracing::error!("pty write error: {:?}", err);
                                    }
                                }
                            } else {
                                break;
                            }
                        }

                        drop(ch_reader);
                        handle.exit_code(0).await?;
                    }
                    Err(err) => {
                        tracing::error!("{:?}", err);
                        handle.exit_code(128).await?;
                    }
                }

                Ok(())
            }
            .instrument(ssh_span),
        );

        Ok(())
    }

    /// 上传文件到服务端工作目录
    #[tracing::instrument(skip(self), err)]
    pub async fn put(&mut self, files: Vec<String>) -> HorseResult<()> {
        tracing::info!("PUT: {}", files.join(" "));

        let env_repo = self.env.get("REPO").context("REPO 环境变量未设置")?;
        let _env_branch = self.env.get("BRANCH").context("BRANCH 环境变量未设置")?;

        let mut repo_path = PathBuf::from(env_repo);
        // 去除开头的 /
        if let Ok(stripped) = repo_path.strip_prefix("/") {
            repo_path = stripped.to_path_buf();
        }

        // 清理路径
        repo_path = repo_path.clean();

        // 裸仓库名称统一添加 .git 后缀
        if repo_path.extension() != Some(OsStr::new("git")) && !repo_path.set_extension("git") {
            tracing::error!("无效仓库路径: {:?}", repo_path);
            let handle = self.handle.take().context("FIXME: NO HANDLE")?;
            handle
                .fail_with_error(
                    2,
                    "HSSH_REPO_PATH_INVALID",
                    format!("无效仓库路径: {:?}", repo_path),
                )
                .await?;
            return Ok(());
        }

        let mut work_path = std::env::current_dir()?.join("workspace").join(repo_path);
        // 构建目录不包含 .git 后缀
        work_path.set_extension("");
        work_path = work_path.clean();

        let remote = files.first().context("FIXME: NO TARGET FILE")?;
        let remote_path = PathBuf::from(remote);
        if remote_path.is_absolute()
            || remote_path
                .components()
                .any(|c| c == std::path::Component::ParentDir)
        {
            let handle = self.handle.take().context("FIXME: NO HANDLE")?;
            handle
                .fail_with_error(
                    2,
                    "HSSH_PUT_PATH_INVALID",
                    format!("非法目标路径: {}", remote),
                )
                .await?;
            return Ok(());
        }

        let target_path = work_path.join(&remote_path).clean();
        if !target_path.starts_with(&work_path) {
            let handle = self.handle.take().context("FIXME: NO HANDLE")?;
            handle
                .fail_with_error(
                    2,
                    "HSSH_PUT_PATH_OUTSIDE_WORKDIR",
                    format!("目标路径超出工作目录: {}", target_path.display()),
                )
                .await?;
            return Ok(());
        }

        let handle = self
            .handle
            .take()
            .context("FIXME: NO HANDLE".color(Color::Red))?;
        let task = self.tm.spawn_handle();
        task.spawn(async move {
            let mut handle = handle;
            let put_res = async {
                if let Some(parent) = target_path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }

                if let Ok(md) = tokio::fs::metadata(&target_path).await {
                    if md.is_dir() {
                        return Err(anyhow!("目标路径是目录: {}", target_path.display()));
                    }
                }

                let mut file = tokio::fs::File::create(&target_path).await?;
                {
                    let mut cin = handle.make_reader();
                    tokio::io::copy(&mut cin, &mut file).await?;
                }
                file.flush().await?;

                tracing::info!("put done: {}", target_path.display());
                handle.exit_code(0).await?;
                Ok::<_, anyhow::Error>(())
            }
            .await;

            if let Err(err) = put_res {
                tracing::error!("put failed: {:?}", err);
                handle
                    .fail_with_error(1, "HSSH_PUT_FAILED", format!("上传失败: {}", err))
                    .await?;
            }

            Ok(())
        });

        Ok(())
    }

    /// 管理员操作（用户、公钥）
    #[tracing::instrument(skip(self), err)]
    pub async fn admin(&mut self, args: Vec<String>) -> HorseResult<()> {
        tracing::info!("ADMIN: {}", args.join(" "));
        let handle = self
            .handle
            .take()
            .context("FIXME: NO HANDLE".color(Color::Red))?;

        if let Err(err) = self.require_admin() {
            handle
                .fail_with_error(3, "HSSH_ADMIN_FORBIDDEN", err.to_string())
                .await?;
            return Ok(());
        }

        let actor = self.user.clone().context("未获取登录用户")?;
        let db = self.db.clone();

        let admin_res: anyhow::Result<String> = async move {
            let section = args.first().map(String::as_str).unwrap_or("");
            let command = args.get(1).map(String::as_str).unwrap_or("");

            let output = match (section, command) {
                ("users", "list") => {
                    let users = User::find().order_by_asc(user::Column::Id).all(&db).await?;
                    let mut rows = Vec::with_capacity(users.len());
                    for user in users {
                        let key_count = user.find_related(SshPk).count(&db).await?;
                        rows.push(AdminUserRow {
                            id: user.id,
                            name: user.name,
                            nick: user.nick,
                            email: user.email,
                            role: user.role,
                            enabled: user.enabled,
                            key_count,
                        });
                    }
                    serde_json::to_string_pretty(&rows)?
                }
                ("users", "add") => {
                    let name = args.get(2).context("用法: users add <name> [admin|user]")?;
                    let role = args
                        .get(3)
                        .map(String::as_str)
                        .unwrap_or("user")
                        .to_ascii_lowercase();
                    if role != "admin" && role != "user" {
                        return Err(anyhow!("角色必须是 admin 或 user"));
                    }

                    let user = user::ActiveModel {
                        name: Set(name.to_string()),
                        role: Set(role),
                        enabled: Set(true),
                        ..Default::default()
                    }
                    .insert(&db)
                    .await?;

                    format!("用户已创建: {} (id={})", user.name, user.id)
                }
                ("users", "enable") => {
                    let name = args.get(2).context("用法: users enable <name>")?;
                    let Some(mut target) = User::find()
                        .filter(user::Column::Name.eq(name.as_str()))
                        .one(&db)
                        .await?
                    else {
                        return Err(anyhow!("用户不存在: {}", name));
                    };

                    if !target.enabled {
                        let mut active: user::ActiveModel = target.clone().into();
                        active.enabled = Set(true);
                        target = active.update(&db).await?;
                    }

                    format!("用户已启用: {}", target.name)
                }
                ("users", "disable") => {
                    let name = args.get(2).context("用法: users disable <name>")?;
                    let Some(mut target) = User::find()
                        .filter(user::Column::Name.eq(name.as_str()))
                        .one(&db)
                        .await?
                    else {
                        return Err(anyhow!("用户不存在: {}", name));
                    };

                    if target.id == actor.id {
                        return Err(anyhow!("不能禁用当前登录管理员"));
                    }

                    if target.enabled && target.is_admin() {
                        let admins = User::find()
                            .filter(user::Column::Role.eq("admin"))
                            .filter(user::Column::Enabled.eq(true))
                            .count(&db)
                            .await?;
                        if admins <= 1 {
                            return Err(anyhow!("不能禁用最后一个启用中的管理员"));
                        }
                    }

                    if target.enabled {
                        let mut active: user::ActiveModel = target.clone().into();
                        active.enabled = Set(false);
                        target = active.update(&db).await?;
                    }

                    format!("用户已禁用: {}", target.name)
                }
                ("users", "role") => {
                    let name = args.get(2).context("用法: users role <name> <admin|user>")?;
                    let role = args.get(3).context("用法: users role <name> <admin|user>")?;
                    let role = role.to_ascii_lowercase();
                    if role != "admin" && role != "user" {
                        return Err(anyhow!("角色必须是 admin 或 user"));
                    }

                    let Some(mut target) = User::find()
                        .filter(user::Column::Name.eq(name.as_str()))
                        .one(&db)
                        .await?
                    else {
                        return Err(anyhow!("用户不存在: {}", name));
                    };

                    if target.role == "admin" && role == "user" && target.enabled {
                        let admins = User::find()
                            .filter(user::Column::Role.eq("admin"))
                            .filter(user::Column::Enabled.eq(true))
                            .count(&db)
                            .await?;
                        if admins <= 1 {
                            return Err(anyhow!("不能降级最后一个启用中的管理员"));
                        }
                    }

                    let mut active: user::ActiveModel = target.clone().into();
                    active.role = Set(role.clone());
                    target = active.update(&db).await?;

                    format!("用户角色已更新: {} => {}", target.name, role)
                }
                ("users", "delete") => {
                    let name = args.get(2).context("用法: users delete <name>")?;
                    let Some(target) = User::find()
                        .filter(user::Column::Name.eq(name.as_str()))
                        .one(&db)
                        .await?
                    else {
                        return Err(anyhow!("用户不存在: {}", name));
                    };

                    if target.id == actor.id {
                        return Err(anyhow!("不能删除当前登录管理员"));
                    }

                    if target.enabled && target.is_admin() {
                        let admins = User::find()
                            .filter(user::Column::Role.eq("admin"))
                            .filter(user::Column::Enabled.eq(true))
                            .count(&db)
                            .await?;
                        if admins <= 1 {
                            return Err(anyhow!("不能删除最后一个启用中的管理员"));
                        }
                    }

                    let id = target.id;
                    let name = target.name.clone();
                    target.delete(&db).await?;
                    format!("用户已删除: {} (id={})", name, id)
                }
                ("keys", "list") => {
                    let keys = if let Some(name) = args.get(2) {
                        let Some(owner) = User::find()
                            .filter(user::Column::Name.eq(name.as_str()))
                            .one(&db)
                            .await?
                        else {
                            return Err(anyhow!("用户不存在: {}", name));
                        };
                        owner.find_related(SshPk).all(&db).await?
                    } else {
                        SshPk::find()
                            .order_by_asc(ssh_pk::Column::UserId)
                            .order_by_asc(ssh_pk::Column::Alg)
                            .all(&db)
                            .await?
                    };

                    let mut rows = Vec::with_capacity(keys.len());
                    for key in keys {
                        let owner = key.find_related(User).one(&db).await?;
                        rows.push(AdminKeyRow {
                            alg: key.alg,
                            key: key.key,
                            user_id: key.user_id,
                            user_name: owner.map(|u| u.name),
                            enabled: key.enabled,
                            comment: key.comment,
                        });
                    }
                    serde_json::to_string_pretty(&rows)?
                }
                ("keys", "add") => {
                    let user_name = args
                        .get(2)
                        .context("用法: keys add <user> <alg> <key> [comment]")?;
                    let alg = args
                        .get(3)
                        .context("用法: keys add <user> <alg> <key> [comment]")?;
                    let key = args
                        .get(4)
                        .context("用法: keys add <user> <alg> <key> [comment]")?;
                    let comment = if args.len() > 5 {
                        Some(args[5..].join(" "))
                    } else {
                        None
                    };

                    let Some(owner) = User::find()
                        .filter(user::Column::Name.eq(user_name.as_str()))
                        .one(&db)
                        .await?
                    else {
                        return Err(anyhow!("用户不存在: {}", user_name));
                    };

                    ssh_pk::ActiveModel {
                        alg: Set(alg.to_string()),
                        key: Set(key.to_string()),
                        user_id: Set(owner.id),
                        enabled: Set(true),
                        comment: Set(comment),
                    }
                    .insert(&db)
                    .await?;

                    format!("公钥已添加到用户: {}", owner.name)
                }
                ("keys", "enable") => {
                    let alg = args.get(2).context("用法: keys enable <alg> <key>")?;
                    let key = args.get(3).context("用法: keys enable <alg> <key>")?;
                    let Some(mut target) = SshPk::find_by_id((alg.to_string(), key.to_string()))
                        .one(&db)
                        .await?
                    else {
                        return Err(anyhow!("公钥不存在"));
                    };

                    if !target.enabled {
                        let mut active: ssh_pk::ActiveModel = target.clone().into();
                        active.enabled = Set(true);
                        target = active.update(&db).await?;
                    }

                    format!("公钥已启用: {} {}", target.alg, target.user_id)
                }
                ("keys", "disable") => {
                    let alg = args.get(2).context("用法: keys disable <alg> <key>")?;
                    let key = args.get(3).context("用法: keys disable <alg> <key>")?;
                    let Some(mut target) = SshPk::find_by_id((alg.to_string(), key.to_string()))
                        .one(&db)
                        .await?
                    else {
                        return Err(anyhow!("公钥不存在"));
                    };

                    if target.enabled {
                        let mut active: ssh_pk::ActiveModel = target.clone().into();
                        active.enabled = Set(false);
                        target = active.update(&db).await?;
                    }

                    format!("公钥已禁用: {} {}", target.alg, target.user_id)
                }
                ("keys", "delete") => {
                    let alg = args.get(2).context("用法: keys delete <alg> <key>")?;
                    let key = args.get(3).context("用法: keys delete <alg> <key>")?;
                    let Some(target) = SshPk::find_by_id((alg.to_string(), key.to_string()))
                        .one(&db)
                        .await?
                    else {
                        return Err(anyhow!("公钥不存在"));
                    };

                    target.delete(&db).await?;
                    "公钥已删除".to_string()
                }
                _ => {
                    return Err(anyhow!(
                        "不支持的 admin 命令, 用法: users|keys <list|add|enable|disable|role|delete> ..."
                    ));
                }
            };
            Ok(output)
        }
        .await;

        match admin_res {
            Ok(output) => {
                let mut cout = handle.make_writer();
                cout.write_all(output.as_bytes()).await?;
                if !output.ends_with('\n') {
                    cout.write_all(b"\n").await?;
                }
                handle.exit_code(0).await?;
            }
            Err(err) => {
                tracing::error!("admin failed: {:?}", err);
                handle
                    .fail_with_error(1, "HSSH_ADMIN_FAILED", err.to_string())
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn ping(&mut self, _args: Vec<String>) -> HorseResult<()> {
        let mut handle = self.handle.take().context("FIXME: NO HANDLE")?;
        let task = self.tm.spawn_handle();

        task.spawn(async move {
            let mut writer = handle.make_writer();
            let mut reader = handle.make_reader();

            let mut head = [0u8; HEAD_SIZE];
            reader.read_exact(&mut head).await?;
            let head = Head::ref_from_bytes(&head).unwrap();

            let mut body = vec![0u8; head.size as usize];
            reader.read_exact(&mut body).await?;
            drop(reader);

            let pong = if let Ok(body) = bincode::deserialize::<Body>(&body) {
                match body {
                    Body::Ping(inst) => {
                        tracing::info!("ping: {:?}", inst.elapsed());
                        Body::Pong(inst)
                    }
                    body => {
                        return Err(anyhow!("不支持的协议: {:?}", body));
                    }
                }
            } else {
                return Err(anyhow!("协议错误: {:?} {:?}", head, body));
            };

            let pong = bincode::serialize(&pong)?;

            writer
                .write_all(v2::head(pong.len() as _).as_bytes())
                .await?;
            writer.write_all(&pong).await?;

            writer.shutdown().await?;
            drop(writer);

            handle.eof().await?;
            handle.close().await?;

            Ok(())
        });

        Ok(())
    }

    pub async fn logs(&mut self, commands: Vec<String>) -> HorseResult<()> {
        use crate::logger::RING_LOG;
        let handle = self.handle.take().context("FIXME: NO HANDLE")?;
        let task = self.tm.spawn_handle();
        let logs = RING_LOG.queue.clone();

        let option_forward = commands.contains(&"-f".to_owned());

        task.spawn(async move {
            let mut writer = handle.make_writer();

            if option_forward {
                loop {
                    let mut channel_closed = false;
                    while let Some(log) = logs.try_pop() {
                        if log.is_empty() {
                            continue;
                        }

                        if writer.write_all(&log).await.is_err() {
                            channel_closed = true;
                            break;
                        }
                    }

                    if channel_closed {
                        break;
                    }

                    tracing::debug!("wait log in 500ms");
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            } else {
                while let Some(log) = logs.try_pop() {
                    if log.is_empty() {
                        continue;
                    }

                    writer.write_all(&log).await?;
                }
            }

            writer.shutdown().await?;
            drop(writer);

            handle.eof().await?;
            handle.close().await?;

            Ok(())
        });

        Ok(())
    }

    pub async fn job(&mut self, command: Vec<String>) -> HorseResult<()> {
        let handle = self.handle.take().context("FIXME: NO HANDLE")?;
        let task = self.tm.spawn_handle();
        let jobs = self.jobs.clone();
        let actor = self.user.clone().context("未获取登录用户")?;

        task.spawn(async move {
            let action = command.first().map(String::as_str).unwrap_or("list");
            match action {
                "list" => {
                    let mut writer = handle.make_writer();
                    let rows = jobs.list_visible(&actor.name, actor.is_admin()).await;
                    let body = serde_json::to_vec_pretty(&rows)?;
                    writer.write_all(&body).await?;
                    writer.write_all(b"\n").await?;
                    writer.shutdown().await?;
                    drop(writer);
                    handle.eof().await?;
                    handle.close().await?;
                    Ok(())
                }
                "attach" => {
                    let mut id = None;
                    let mut follow = true;
                    for arg in command.iter().skip(1) {
                        if arg == "--no-follow" {
                            follow = false;
                            continue;
                        }

                        if arg.starts_with("--") {
                            handle
                                .fail_with_error(
                                    2,
                                    "HSSH_JOB_BAD_REQUEST",
                                    format!(
                                        "不支持的参数: {arg}, 用法: attach [job_id] [--no-follow]"
                                    ),
                                )
                                .await?;
                            return Ok(());
                        }

                        if id.is_none() {
                            id = Some(arg.clone());
                            continue;
                        }

                        handle
                            .fail_with_error(
                                2,
                                "HSSH_JOB_BAD_REQUEST",
                                format!("不支持的参数: {arg}, 用法: attach [job_id] [--no-follow]"),
                            )
                            .await?;
                        return Ok(());
                    }

                    let id = if let Some(id) = id.filter(|id| !id.trim().is_empty()) {
                        id
                    } else {
                        let mut running = jobs
                            .list_visible(&actor.name, actor.is_admin())
                            .await
                            .into_iter()
                            .filter(|job| job.running)
                            .collect::<Vec<_>>();

                        match running.len() {
                            0 => {
                                handle
                                    .fail_with_error(2, "HSSH_JOB_NOT_FOUND", "没有运行中的任务")
                                    .await?;
                                return Ok(());
                            }
                            1 => running.swap_remove(0).id,
                            _ => {
                                let mut writer = handle.make_writer();
                                let body = serde_json::to_vec_pretty(&running)?;
                                writer.write_all(&body).await?;
                                writer.write_all(b"\n").await?;
                                drop(writer);

                                handle
                                    .fail_with_error(
                                        2,
                                        "HSSH_JOB_AMBIGUOUS",
                                        "存在多个运行中的任务，请指定 job_id",
                                    )
                                    .await?;
                                return Ok(());
                            }
                        }
                    };

                    let Some(job) = jobs.get_visible(&id, &actor.name, actor.is_admin()).await
                    else {
                        handle
                            .fail_with_error(2, "HSSH_JOB_NOT_FOUND", format!("未找到任务: {id}"))
                            .await?;
                        return Ok(());
                    };

                    let mut writer = handle.make_writer();
                    let (snapshot, exit_code, _, dropped_bytes) = job.snapshot().await;
                    if dropped_bytes > 0 {
                        writer
                            .write_all(format!("[JOB] dropped_bytes={dropped_bytes}\n").as_bytes())
                            .await?;
                    }
                    if !snapshot.is_empty() {
                        writer.write_all(&snapshot).await?;
                    }

                    if follow && exit_code.is_none() {
                        let mut rx = job.subscribe();
                        loop {
                            match rx.recv().await {
                                Ok(JobEvent::Output(data)) => {
                                    writer.write_all(&data).await?;
                                }
                                Ok(JobEvent::Done(code)) => {
                                    writer
                                        .write_all(format!("\n[JOB] exit_code={code}\n").as_bytes())
                                        .await?;
                                    break;
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                    break;
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                    writer
                                        .write_all(
                                            format!("\n[JOB] lagged_frames={n}\n").as_bytes(),
                                        )
                                        .await?;
                                }
                            }
                        }
                    } else if let Some(code) = exit_code {
                        writer
                            .write_all(format!("\n[JOB] exit_code={code}\n").as_bytes())
                            .await?;
                    }

                    writer.shutdown().await?;
                    drop(writer);
                    handle.eof().await?;
                    handle.close().await?;
                    Ok(())
                }
                other => {
                    handle
                        .fail_with_error(
                            2,
                            "HSSH_JOB_BAD_REQUEST",
                            format!("不支持的 job 命令: {other}"),
                        )
                        .await?;
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
    #[tracing::instrument(skip(self), err)]
    pub async fn just(&mut self, command: Vec<String>) -> HorseResult<()> {
        tracing::info!("[just] {}", command.join(" "));
        let env_repo = self
            .env
            .get("REPO")
            .cloned()
            .context("REPO 环境变量未设置")?;
        let env_branch = self
            .env
            .get("BRANCH")
            .cloned()
            .context("BRANCH 环境变量未设置")?;
        let justfile = self.env.get("JUSTFILE").cloned();

        let mut repo_path = PathBuf::from(&env_repo);
        // 去除开头的 /
        if let Ok(stripped) = repo_path.strip_prefix("/") {
            repo_path = stripped.to_path_buf();
        }

        // 清理路径
        repo_path = repo_path.clean();
        // 工作路径不包含 .git
        let repo_work_path = repo_path.clone();

        let mut handle = self.handle.take().context("FIXME: NO HANDLE")?;

        if let Some(fst) = repo_path.components().next() {
            // 如果提供的地址包含 .. 等路径，则拒绝请求
            if fst == std::path::Component::ParentDir {
                tracing::warn!("拒绝仓库请求, 路径不合法: {}", repo_path.display());
                handle
                    .error(format!("拒绝仓库请求, 路径不合法: {}", repo_path.display()))
                    .await?;
                handle.eof().await?;
                handle.close().await?;
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
            handle
                .error(format!("无效仓库路径: {:?}", repo_path))
                .await?;
            handle.eof().await?;
            handle.close().await?;
            return Ok(());
        }

        let repo = Repo::from(&repo_path);
        tracing::info!("GIT REPO: {}", repo.path().display());
        let task = self.tm.spawn_handle();
        let command_line = command.join(" ");
        let owner = self.user_name().to_string();
        let job = self
            .jobs
            .create_job(owner, "just", command_line.clone())
            .await;

        // 1. 检出代码用于构建
        // 2. 执行项目的 just 命令, 项目必须包含 justfile 文件

        // 如果仓库目录不存在
        if !repo.exists() {
            handle.error("代码仓库不存在, 请先 push 代码").await?;
            handle.eof().await?;
            handle.close().await?;
            return Ok(());
        }

        let mut work_path = std::env::current_dir()?
            .join("workspace")
            .join(repo_work_path);
        // 构建目录不包含 .git 后缀
        work_path.set_extension("");

        if !work_path.exists() {
            tracing::info!("CREATE DIR: {}", work_path.display());
            std::fs::create_dir_all(&work_path).context("创建工作目录失败")?;
        }

        // 执行命令
        handle.info("检出代码到工作目录...").await?;
        handle.info(format!("当前仓库: {}", env_repo)).await?;
        handle.info(format!("检出分支: {}", env_branch)).await?;
        handle.info(format!("job_id={}", job.id())).await?;

        if let Err(err) = repo
            .checkout(&work_path, Some(&env_branch))
            .await
            .context("检出代码失败")
        {
            tracing::error!("{:?}", err);
            handle.error(err.to_string()).await?;
            handle.eof().await?;
            handle.close().await?;
            return Ok(());
        }

        let just_span = tracing::info_span!("just");
        let env = self.env.clone();
        task.spawn(async move {
            let mut final_code = 1_i32;
            let result: anyhow::Result<()> = async {
                let mut diff_input = handle.make_reader();
                let mut buf = vec![];
                diff_input.read_to_end(&mut buf).await?;

                repo.apply(&work_path, &buf).await.context("git apply")?;
                drop(diff_input);

                handle
                    .info(format!("just {}...", command.join(" ")).bold().to_string())
                    .await?;

                let mut cmd = Command::new("just");
                cmd.envs(&env);

                #[cfg(target_os = "windows")]
                {
                    #[allow(unused_imports)]
                    use std::os::windows::process::CommandExt;
                    const CREATE_NO_WINDOW: u32 = 0x08000000;

                    cmd.creation_flags(CREATE_NO_WINDOW);
                }

                cmd.current_dir(&work_path);

                // user defined justfile
                if let Some(justfile) = justfile {
                    cmd.arg("-f");
                    cmd.arg(justfile);
                } else {
                    // justfile.<os>
                    let justfile = format!("justfile.{}", std::env::consts::OS);
                    if let Ok(true) = std::fs::exists(&justfile) {
                        cmd.arg("-f");
                        cmd.arg(justfile);
                    }
                }

                cmd.arg("--color=always");
                cmd.args(command);

                cmd.kill_on_drop(true);
                cmd.stdout(Stdio::piped());
                cmd.stderr(Stdio::piped());

                let mut cmd = match cmd.spawn() {
                    Ok(cmd) => cmd,
                    Err(err) => {
                        final_code = 127;
                        handle
                            .fail_with_error(
                                127,
                                "HSSH_JUST_SPAWN_FAILED",
                                format!("spawn: `just` failed: {err}"),
                            )
                            .await?;
                        return Ok(());
                    }
                };

                let mut stdout = cmd.stdout.take().unwrap();
                let mut stderr = cmd.stderr.take().unwrap();

                let err_job = job.clone();
                let mut o_output = handle.make_writer();
                let err_fut = async move {
                    let mut buf = [0u8; 1024];
                    while let Ok(len) = stderr.read(&mut buf).await {
                        if len == 0 {
                            break;
                        }

                        o_output.write_all(&buf[..len]).await?;
                        o_output.flush().await?;
                        err_job.append_output(&buf[..len]).await;
                    }
                    Ok::<_, HorseError>(())
                };

                let out_job = job.clone();
                let mut o_output = handle.make_writer();
                let out_fut = async move {
                    let mut buf = [0u8; 1024];
                    while let Ok(len) = stdout.read(&mut buf).await {
                        if len == 0 {
                            break;
                        }

                        o_output.write_all(&buf[..len]).await?;
                        o_output.flush().await?;
                        out_job.append_output(&buf[..len]).await;
                    }
                    Ok::<_, HorseError>(())
                };

                futures::future::try_join(out_fut, err_fut).await?;

                let exit_status = cmd.wait().await?;
                final_code = exit_status.code().unwrap_or(128);
                if exit_status.success() {
                    handle.info("构建完成").await?;
                } else {
                    handle.error("构建失败").await?;
                }

                handle.exit(exit_status).instrument(just_span).await?;
                Ok(())
            }
            .await;

            job.finish(final_code).await;
            result
        });

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
    /// ssh -o SetEnv="REPO=workhorse BRANCH=main CARGO_BUILD=yyy" cargo@xxx.xxx.xxx.xxx -- build
    /// ```
    #[tracing::instrument(skip(self), err)]
    pub async fn cargo(&mut self, command: Vec<String>) -> HorseResult<()> {
        let env_repo = self
            .env
            .get("REPO")
            .context("REPO 环境变量未设置")?
            .to_owned();
        let env_branch = self
            .env
            .get("BRANCH")
            .context("BRANCH 环境变量未设置")?
            .to_owned();

        let mut repo_path = std::env::current_dir()?.join("repos").join(&env_repo);
        repo_path.set_extension("git");
        repo_path = repo_path.clean();

        let mut handle = self.handle.take().context("FIXME: NO HANDLE").unwrap();
        let task = self.tm.spawn_handle();
        let repo = Repo::from(repo_path);
        let command_line = command.join(" ");
        let cargo_action = format!(
            "cargo.{}",
            command.first().map(String::as_str).unwrap_or("unknown")
        );
        let owner = self.user_name().to_string();
        let job = self
            .jobs
            .create_job(owner, cargo_action, command_line.clone())
            .await;
        handle.info(format!("job_id={}", job.id())).await?;

        if !repo.exists() {
            tracing::error!("仓库不存在: {}", repo.path().display());
            handle.error("仓库不存在").await?;
            handle.eof().await?;
            handle.close().await?;
            return Ok(());
        }

        let mut work_path = std::env::current_dir()?.join("workspace").join(&env_repo);
        // 构建目录不包含 .git 后缀
        work_path.set_extension("");
        work_path = work_path.clean();

        if !work_path.exists() {
            std::fs::create_dir_all(&work_path).context("创建工作目录失败")?;
        }

        // let work_repo = Repo::clone(repo.path(), work_path, Some(env_branch))
        //     .await
        //     .context("克隆仓库失败")?;

        // cargo 参数
        let env_cargo_options = self
            .env
            .get("CARGO_OPTIONS")
            .context("CARGO_OPTIONS 环境变量未设置")?;
        // 是否使用 zigbuild
        let env_zigbuild = self
            .env
            .get("ZIGBUILD")
            .map(|s| s.parse::<bool>().unwrap_or(false))
            .unwrap_or(false);

        if env_zigbuild {
            tracing::info!("[cargo] zigbuild {}", command.join(" "));
        } else {
            tracing::info!("[cargo] {}", command.join(" "));
        }

        // 构建命令, 支持 cargo zigbuild
        let mut cmd = match command.first().context("FIXME: CARGO COMMAND")?.as_str() {
            // cargo build
            "zigbuild" | "build" => {
                cargo_command!(build, env_cargo_options, env_zigbuild)
            }
            "check" => {
                cargo_command!(check, env_cargo_options)
            }
            "clean" => {
                cargo_command!(clean, env_cargo_options)
            }
            "clippy" => {
                cargo_command!(clippy, env_cargo_options)
            }
            "doc" => {
                cargo_command!(doc, env_cargo_options)
            }
            "install" => {
                cargo_command!(install, env_cargo_options)
            }
            "metadata" => {
                cargo_command!(metadata, env_cargo_options)
            }
            "run" => {
                cargo_command!(run, env_cargo_options)
            }
            "rustc" => {
                cargo_command!(rustc, env_cargo_options)
            }
            "test" => {
                cargo_command!(test, env_cargo_options)
            }
            _ => {
                tracing::warn!("未实现的 cargo 命令: {}", command.join(" "));
                handle
                    .error(format!("未实现的 cargo 命令: {}", command.join(" ")))
                    .await?;
                handle.eof().await?;
                handle.close().await?;
                return Ok(());
            }
        };

        #[cfg(target_os = "windows")]
        {
            #[allow(unused_imports)]
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;

            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        cmd.envs(&self.env);
        cmd.kill_on_drop(true);
        cmd.current_dir(&work_path);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let cargo_span = tracing::info_span!("cargo", command = ?cmd);
        task.spawn(async move {
            let mut final_code = 1_i32;
            let result: anyhow::Result<()> = async {
                let mut o_output = handle.make_writer();
                let mut e_output = handle.make_writer();

                // git checkout
                repo.checkout(&work_path, Some(env_branch.as_str()))
                    .await
                    .context("git checkout")?;
                // git apply
                let mut diff_input = handle.make_reader();
                let mut buf = vec![];
                diff_input.read_to_end(&mut buf).await?;

                repo.apply(&work_path, &buf).await.context("git apply")?;
                drop(diff_input);

                // Run the command
                let mut cmd = match cmd.spawn() {
                    Ok(cmd) => cmd,
                    Err(err) => {
                        final_code = 127;
                        handle
                            .fail_with_error(
                                127,
                                "HSSH_CARGO_SPAWN_FAILED",
                                format!("spawn: `cargo` failed: {err}"),
                            )
                            .await?;
                        return Ok(());
                    }
                };
                let mut stdout = cmd.stdout.take().unwrap();
                let mut stderr = cmd.stderr.take().unwrap();
                let out_job = job.clone();
                let err_job = job.clone();

                let o_fut = copy_with_job(&mut stdout, &mut o_output, out_job);
                let e_fut = copy_with_job(&mut stderr, &mut e_output, err_job);

                futures::future::try_join(o_fut, e_fut).await?;

                e_output.shutdown().await.context("shutdown e_output")?;
                o_output.shutdown().await.context("shutdown o_output")?;

                let status = cmd.wait().await?;
                final_code = status.code().unwrap_or(128);
                handle.exit(status).instrument(cargo_span).await?;
                Ok(())
            }
            .await;

            job.finish(final_code).await;
            result
        });

        Ok(())
    }

    pub async fn apply(&mut self, _command: Vec<String>) -> HorseResult<()> {
        let env_repo = self.env.get("REPO").context("REPO 环境变量未设置")?;
        let _env_branch = self.env.get("BRANCH").context("BRANCH 环境变量未设置")?;

        let mut repo_path = std::env::current_dir()?.join("repos").join(env_repo);
        repo_path.set_extension("git");
        repo_path = repo_path.clean();

        let mut handle = self.handle.take().context("FIXME: NO HANDLE").unwrap();
        let task = self.tm.spawn_handle();
        let repo = Repo::from(repo_path);

        if !repo.exists() {
            tracing::error!("仓库不存在: {}", repo.path().display());
            handle.error("仓库不存在").await?;
            return Ok(());
        }

        let mut work_path = std::env::current_dir()?.join("workspace").join(env_repo);
        // 构建目录不包含 .git 后缀
        work_path.set_extension("");
        work_path = work_path.clean();

        if !work_path.exists() {
            std::fs::create_dir_all(&work_path).context("创建工作目录失败")?;
        }

        // 检出最新代码 (HEAD)
        // repo.checkout(&work_path, Some(env_branch)).await?;

        let mut cmd = tokio::process::Command::new("git");

        #[cfg(target_os = "windows")]
        {
            #[allow(unused_imports)]
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;

            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        cmd.kill_on_drop(true);
        cmd.current_dir(&work_path);
        cmd.stdin(std::process::Stdio::piped());

        cmd.arg("apply");

        task.spawn(async move {
            // Run the command
            let mut cmd = cmd.spawn().context("spawn: `git`")?;

            {
                let mut cin = handle.make_reader();
                let mut stdin = cmd.stdin.take().unwrap();

                while let Ok(len) = tokio::io::copy(&mut cin, &mut stdin).await {
                    // eof
                    if len == 0 {
                        break;
                    }
                }

                tracing::info!("[git] apply done");
            }

            let cmd = cmd.wait_with_output().await?;
            if !cmd.status.success() {
                let err = String::from_utf8_lossy(&cmd.stderr);
                tracing::error!("git apply err: {err}");
            }

            handle.exit(cmd.status).await?;
            Ok(())
        });

        Ok(())
    }
}

#[async_trait::async_trait]
impl Server for AppServer {
    type Handler = Self;

    /// 创建新连接
    fn new_client(&mut self, peer: Option<std::net::SocketAddr>) -> Self {
        tracing::info!("新建连接: {:?}", peer);
        let this = self.clone();
        self.id += 1;
        this
    }

    #[tracing::instrument(skip_all, level = "debug")]
    async fn run_on_socket(
        &mut self,
        config: Arc<Config>,
        socket: &TcpListener,
    ) -> Result<(), std::io::Error> {
        if config.maximum_packet_size > 65535 {
            tracing::error!(
                "Maximum packet size ({:?}) should not larger than a TCP packet (65535)",
                config.maximum_packet_size
            );
        }

        let mut tm = TaskManager::default();
        let handle = tm.spawn_handle();

        loop {
            match socket.accept().await {
                Ok((socket, _)) => {
                    let config = config.clone();
                    let handler = self.new_client(socket.peer_addr().ok());

                    let span = tracing::info_span!("socket.accept", socket=?socket.peer_addr());
                    handle.spawn(
                        async move {
                            tracing::debug!("handle-socket");
                            let session = match run_stream(config, socket, handler).await {
                                Ok(s) => s,
                                Err(e) => {
                                    tracing::error!("session-setup-failed: {:?}", e);
                                    return Ok(());
                                }
                            };

                            session.await?;
                            Ok(())
                        }
                        .instrument(span),
                    );
                }

                // 1. Too many open files
                //    enlarge your `ulimit -n` number
                err => {
                    tracing::error!("accept-error: {:?}", err);
                    break;
                }
            }
        }

        tm.terminate();

        Ok(())
    }
}

#[async_trait::async_trait]
impl Handler for AppServer {
    type Error = HorseError;

    #[tracing::instrument(skip_all, fields(channel=%channel.id()))]
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
    #[tracing::instrument(skip(self))]
    async fn auth_password(&mut self, action: &str, password: &str) -> Result<Auth, Self::Error> {
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
    #[tracing::instrument(skip(self, pk))]
    async fn auth_publickey_offered(
        &mut self,
        action: &str,
        pk: &PublicKey,
    ) -> Result<Auth, Self::Error> {
        tracing::info!("PubKey: {:?}", pk.to_openssh());
        Ok(Auth::Accept)
    }

    /// Check authentication using the "publickey" method. This method
    /// is called after the signature has been verified and key
    /// ownership has been confirmed.
    /// Russh guarantees that rejection happens in constant time
    /// `config.auth_rejection_time`, except if this method takes more
    /// time than that.
    #[tracing::instrument(skip(self, pk))]
    async fn auth_publickey(&mut self, action: &str, pk: &PublicKey) -> HorseResult<Auth> {
        #[allow(deprecated)]
        let data = base64::encode(&pk.to_bytes().context("pk bytes")?);

        let Some(sa) = SshPk::find_by_id((pk.algorithm().to_string(), data.to_owned()))
            .one(&self.db)
            .await?
        else {
            tracing::error!("公钥未记录: ({} {})", pk.algorithm().to_string(), data);
            return Ok(Auth::Reject {
                proceed_with_methods: Some(MethodSet::PUBLICKEY),
            });
        };

        if !sa.enabled {
            tracing::warn!("公钥已禁用: ({} {})", pk.algorithm().to_string(), data);
            return Ok(Auth::Reject {
                proceed_with_methods: Some(MethodSet::PUBLICKEY),
            });
        }

        let Some(user) = sa.find_related(User).one(&self.db).await? else {
            tracing::error!("公钥未授权: ({} {})", pk.algorithm().to_string(), data);
            return Ok(Auth::Reject {
                proceed_with_methods: Some(MethodSet::PUBLICKEY),
            });
        };

        if !user.enabled {
            tracing::warn!("用户已禁用: {}", user.name);
            return Ok(Auth::Reject {
                proceed_with_methods: Some(MethodSet::PUBLICKEY),
            });
        }

        self.action = action.to_string();
        self.user.replace(SessionUser {
            id: user.id,
            name: user.name.clone(),
            role: user.role.clone(),
        });

        tracing::info!("Login As: {} ({})", user.name, user.role);
        Ok(Auth::Accept)
    }

    /// Check authentication using an OpenSSH certificate. This method
    /// is called after the signature has been verified and key
    /// ownership has been confirmed.
    /// Russh guarantees that rejection happens in constant time
    /// `config.auth_rejection_time`, except if this method takes more
    /// time than that.
    #[tracing::instrument(skip(self, _certificate))]
    async fn auth_openssh_certificate(
        &mut self,
        user: &str,
        _certificate: &Certificate,
    ) -> Result<Auth, Self::Error> {
        Ok(Auth::Reject {
            proceed_with_methods: None,
        })
    }

    /// The client requests an X11 connection.
    #[allow(unused)]
    #[tracing::instrument(skip(self, session))]
    async fn x11_request(
        &mut self,
        channel: ChannelId,
        single_connection: bool,
        x11_auth_protocol: &str,
        x11_auth_cookie: &str,
        x11_screen_number: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // session.channel_success(channel);
        Ok(())
    }

    /// The client wants to set the given environment variable. Check
    /// these carefully, as it is dangerous to allow any variable
    /// environment to be set.
    #[allow(unused)]
    #[tracing::instrument(skip_all)]
    async fn env_request(
        &mut self,
        channel: ChannelId,
        key: &str,
        value: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let key = key.to_uppercase();
        if self.debug_enabled() || key == "HORSE_TRACE_ID" {
            if key == "HORSE_TRACE_ID" {
                tracing::info!(
                    trace_id = value,
                    key = key.as_str(),
                    stage = "env.set",
                    "stage"
                );
            } else {
                tracing::info!(key = key.as_str(), stage = "env.set", "stage");
            }
        }
        self.env.insert(key, value.to_string());
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

    #[cfg(not(windows))]
    /// The client's window size has changed.
    #[tracing::instrument(skip(self, session, channel))]
    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        col_width: u32,
        row_height: u32,
        pixel_width: u32,
        pixel_height: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("window change: {}x{}", col_width, row_height);
        let clients = self.clients.lock().await;
        if let Some((pty, _)) = clients.get(&self.id) {
            pty.resize(Size::new(col_width as _, row_height as _))
                .context("resize pty")?;
        }

        session.channel_success(channel)?;
        Ok(())
    }

    #[cfg(windows)]
    /// The client's window size has changed.
    #[tracing::instrument(skip(self, session, channel))]
    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        cols: u32,
        rows: u32,
        pixel_width: u32,
        pixel_height: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("window change: {}x{}", cols, rows);
        let clients = self.clients.lock().await;
        if let Some(pty) = clients.get(&self.id) {
            if let Err(err) = pty.set_size(cols as _, rows as _) {
                tracing::error!("change size err: {:?}", err);
            }
        }

        session.channel_success(channel)?;
        Ok(())
    }

    #[cfg(not(windows))]
    #[allow(unused)]
    #[tracing::instrument(skip(self, session, modes), fields(os = std::env::consts::OS))]
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("pty request: {}x{}", col_width, row_height);

        let (pty, pts) = pty_process::open().context("open pty")?;
        pty.resize(Size::new(row_height as _, col_width as _))
            .context("resize pty")?;

        let mut clients = self.clients.lock().await;
        clients.insert(self.id, (pty, pts));

        session.channel_success(channel)?;
        Ok(())
    }

    #[cfg(windows)]
    #[allow(unused)]
    #[tracing::instrument(skip(self, session, modes), fields(os = std::env::consts::OS))]
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        term: &str,
        cols: u32,
        rows: u32,
        pix_width: u32,
        pix_height: u32,
        modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        use std::ffi::OsString;
        use winptyrs::{AgentConfig, MouseMode, PTYArgs, PTYBackend, PTY};
        tracing::info!("pty request: {}x{}", cols, rows);

        let pty_args = PTYArgs {
            cols: cols as _,
            rows: rows as _,
            mouse_mode: MouseMode::WINPTY_MOUSE_MODE_NONE,
            timeout: 10000,
            agent_config: AgentConfig::WINPTY_FLAG_COLOR_ESCAPES,
        };

        match PTY::new_with_backend(&pty_args, PTYBackend::ConPTY) {
            Ok(pty) => {
                let mut clients = self.clients.lock().await;
                clients.insert(self.id, pty);

                session.channel_success(channel)?;
            }
            Err(err) => {
                tracing::error!("winpty failed: {:?}", err);
                session.channel_failure(channel)?;
            }
        };

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
        let command_line = command.join(" ");
        let started = std::time::Instant::now();
        if self.debug_enabled() {
            tracing::info!(
                trace_id = %self.trace_id(),
                action = self.action.as_str(),
                user = self.user_name(),
                stage = "dispatch.start",
                command = command_line.as_str(),
                "stage"
            );
        }

        let dispatch_res = match self.action.as_str() {
            "health" => self.health(command).await,
            "ping" => self.ping(command).await,
            "logs" => self.logs(command).await,
            "cargo" => self.cargo(command).await,
            "apply" => self.apply(command).await,
            // just 命令支持 just.xxx 格式, xxx 对应 justfile 中的运行指令
            "just" => self.just(command).await,
            // action if action.starts_with("just") => {
            //     let mut subaction = action.split(".").skip(1).collect::<Vec<_>>().join(".");
            //     if subaction.is_empty() {
            //         subaction = "build".to_owned();
            //     }
            //     self.just(command, subaction).await?;
            // }
            "git" => self.git(command).await,
            "cmd" => self.cmd(command).await,
            "get" => self.get(command).await,
            "scp" => self.scp(command).await,
            "put" => self.put(command).await,
            "admin" => self.admin(command).await,
            "job" => self.job(command).await,
            "ssh" => self.ssh(command).await,
            action => {
                let handle = self.handle.take().context("FIXME: NO HANDLE").unwrap();
                handle
                    .error_with_code("HSSH_ACTION_UNSUPPORTED", format!("不支持的命令: {action}"))
                    .await?;
                if self.debug_enabled() {
                    tracing::warn!(
                        trace_id = %self.trace_id(),
                        action = self.action.as_str(),
                        user = self.user_name(),
                        stage = "dispatch.unsupported",
                        command = command_line.as_str(),
                        elapsed_ms = started.elapsed().as_millis(),
                        "stage"
                    );
                }
                session.channel_failure(channel_id)?;
                return Ok(());
            }
        };

        if let Err(err) = dispatch_res {
            if self.debug_enabled() {
                tracing::error!(
                    trace_id = %self.trace_id(),
                    action = self.action.as_str(),
                    user = self.user_name(),
                    stage = "dispatch.error",
                    command = command_line.as_str(),
                    elapsed_ms = started.elapsed().as_millis(),
                    error = %err,
                    "stage"
                );
            }
            return Err(err);
        }

        if self.debug_enabled() {
            tracing::info!(
                trace_id = %self.trace_id(),
                action = self.action.as_str(),
                user = self.user_name(),
                stage = "dispatch.done",
                command = command_line.as_str(),
                elapsed_ms = started.elapsed().as_millis(),
                "stage"
            );
        }
        session.channel_success(channel_id)?;

        Ok(())
    }

    /// Used for reverse-forwarding ports, see
    /// [RFC4254](https://tools.ietf.org/html/rfc4254#section-7).
    /// If `port` is 0, you should set it to the allocated port number.
    #[tracing::instrument(skip(self, session), level = "info")]
    async fn tcpip_forward(
        &mut self,
        address: &str,
        port: &mut u32,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let tcpip_forward_span = tracing::info_span!("tcpip-forward");
        let task = self.tm.spawn_handle();
        let address = address.to_string();
        let port = *port;

        tracing::info!("forwarding: {}", address);
        let addr = format!("{}:{}", address, port);

        let listener = match TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(err) => {
                tracing::error!("bind error: {:?}", err);
                return Ok(false);
            }
        };
        tracing::info!("bind success");
        let socket_task = task.clone();
        let handle = session.handle();

        task.spawn(
            async move {
                // listen on server
                while let Ok((mut stream, peer)) = listener.accept().await {
                    let handle = handle.clone();
                    let address = address.clone();
                    socket_task.spawn(async move {
                        tracing::info!("accepted from {}", peer);
                        match handle
                            .channel_open_forwarded_tcpip(
                                &address,
                                port,
                                peer.ip().to_string(),
                                peer.port() as _,
                            )
                            .await
                        {
                            Err(err) => {
                                tracing::error!("channel-open-forwarded-tcpip error: {:?}", err);
                            }
                            Ok(mut channel) => {
                                tracing::info!("open channel");
                                let mut ch_writer = channel.make_writer();
                                let mut ch_reader = channel.make_reader();
                                let (mut reader, mut writer) = stream.split();

                                let writer_fut = tokio::io::copy(&mut reader, &mut ch_writer);
                                let reader_fut = tokio::io::copy(&mut ch_reader, &mut writer);

                                futures::future::try_join(writer_fut, reader_fut).await?;
                                tracing::info!("done");
                            }
                        }

                        Ok(())
                    });
                }
                Ok(())
            }
            .instrument(tcpip_forward_span),
        );

        Ok(true)
    }

    /// Used to stop the reverse-forwarding of a port, see
    /// [RFC4254](https://tools.ietf.org/html/rfc4254#section-7).
    #[tracing::instrument(skip(self, session), level = "info")]
    #[allow(unused)]
    async fn cancel_tcpip_forward(
        &mut self,
        address: &str,
        port: u32,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        tracing::info!("cancel");
        Ok(true)
    }

    /// Called when a new TCP/IP is created.
    /// Return value indicates whether the channel request should be granted.
    #[allow(unused_variables)]
    #[tracing::instrument(skip(self, session, channel), level = "info", fields(channel=%channel.id()))]
    async fn channel_open_direct_tcpip(
        &mut self,
        mut channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        originator_address: &str,
        originator_port: u32,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let task = self.tm.spawn_handle();
        let host_to_connect = host_to_connect.to_string();
        let tcpip_span = tracing::info_span!("connect");

        let socket = TcpSocket::new_v4()?;
        let Ok(mut stream) = socket
            .connect(
                format!("{}:{}", host_to_connect, port_to_connect)
                    .parse()
                    .context("addr parse")?,
            )
            .await
        else {
            return Ok(false);
        };

        let task = self.tm.spawn_handle();

        task.spawn(
            async move {
                tracing::info!("success");
                let (mut reader, mut writer) = stream.split();

                let mut ch_writer = channel.make_writer();
                let mut ch_reader = channel.make_reader();

                let reader_fut = tokio::io::copy(&mut reader, &mut ch_writer);
                let writer_fut = tokio::io::copy(&mut ch_reader, &mut writer);

                futures::future::try_join(reader_fut, writer_fut).await?;

                tracing::info!("done");
                ch_writer.shutdown().await?;
                drop(ch_reader);

                channel.eof().await?;
                Ok(())
            }
            .instrument(tcpip_span),
        );

        Ok(true)
    }

    /// Called when a new forwarded connection comes in.
    /// <https://www.rfc-editor.org/rfc/rfc4254#section-7>
    #[allow(unused_variables)]
    #[tracing::instrument(skip(self, session), level = "info")]
    async fn channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        originator_address: &str,
        originator_port: u32,
        session: &mut Session,
    ) -> Result<bool, Self::Error> {
        tracing::info!("open");
        Ok(true)
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
        _channel_id: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> HorseResult<()> {
        tracing::debug!("Recv Data: {}", data.len());
        Ok(())
    }

    /// 当客户端传输结束时调用。
    async fn channel_eof(
        &mut self,
        _channel_id: ChannelId,
        _session: &mut Session,
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
    #[tracing::instrument(skip(self), name = "AppServer::drop", level = "debug")]
    fn drop(&mut self) {
        tracing::debug!("cleanup");
    }
}

pub async fn run() -> HorseResult<()> {
    let key = key_init();
    let config = Config {
        inactivity_timeout: None,
        auth_rejection_time: std::time::Duration::from_secs(1),
        auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
        keys: vec![key],
        keepalive_interval: Some(std::time::Duration::from_secs(5)),
        ..Default::default()
    };

    let mut server = AppServer::new(DB.clone());
    server
        .run(config, ("0.0.0.0", 2222))
        .await
        .expect("Failed running server");
    Ok(())
}
