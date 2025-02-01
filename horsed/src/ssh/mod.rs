use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Stdio;
use std::str::from_utf8;
use std::sync::Arc;

use crate::db::entity::prelude::{SshPk, User};
use crate::git::repo::Repo;
use crate::prelude::*;
use anyhow::Context;
use clean_path::Clean;
use colored::{Color, Colorize};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use russh::keys::{Certificate, PublicKey};
use russh::{server::*, MethodSet};
use russh::{Channel, ChannelId, Sig};
use sea_orm::{DatabaseConnection, EntityTrait, ModelTrait};
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
use tokio::process::Command;
use tokio::sync::Mutex;

mod handle;
pub mod setup;
use handle::ChannelHandle;
use v2::Body;

struct AppServer {
    /// 一些共享数据
    clients: Arc<Mutex<HashMap<ChannelId, ChannelHandle>>>,
    /// 任务管理器
    tm: TaskManager,
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
            tm: TaskManager::default(),
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
            tm: TaskManager::default(),
            handle: None,
            db: DB.clone(),
            action: String::new(),
            env: HashMap::new(),
        }
    }

    pub async fn run(&mut self) -> HorseResult<()> {
        let key = key_init();
        let config = Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(3600)),
            auth_rejection_time: std::time::Duration::from_secs(1),
            auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
            keys: vec![key],
            keepalive_interval: Some(std::time::Duration::from_secs(5)),
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
    pub async fn cmd(&mut self, command: Vec<String>) -> HorseResult<()> {
        tracing::info!("CMD: {}", command.join(" "));

        let env_repo = self.env.get("REPO");
        let env_branch = self.env.get("BRANCH");

        // 如果命令中包含 REPO 或者 BRANCH 环境变量, 则切换到工作目录执行命令
        let cmd_dir = if let (Some(env_repo), Some(_)) = (env_repo, env_branch) {
            let mut repo_path = PathBuf::from(env_repo);
            // 去除开头的 /
            if repo_path.starts_with("/") {
                repo_path.strip_prefix("/").context("REPO STRIP_PREFIX")?;
            }

            // 清理路径
            repo_path = repo_path.clean();

            // 裸仓库名称统一添加 .git 后缀
            if repo_path.extension() != Some(OsStr::new("git")) && !repo_path.set_extension("git") {
                tracing::error!("无效仓库路径: {:?}", repo_path);
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

        let mut handle = self
            .handle
            .take()
            .context("FIXME: NO HANDLE".color(Color::Red))?;
        let task = self.tm.spawn_handle();

        task.spawn(async move {
            #[cfg(windows)]
            match handle
                .exec(
                    Command::new("cmd.exe")
                        .current_dir(&cmd_dir)
                        .arg("/C")
                        .args(command),
                )
                .await
            {
                Ok(mut cmd) => {
                    handle.exit(cmd.wait().await?).await?;
                }
                Err(err) => {
                    tracing::error!("command failed: {}", err);
                }
            }
            #[cfg(not(windows))]
            match handle
                .exec(
                    Command::new("sh")
                        .current_dir(&cmd_dir)
                        .arg("-c")
                        .arg(command.join(" ")),
                )
                .await
            {
                Ok(mut cmd) => {
                    handle.exit(cmd.wait().await?).await?;
                }
                Err(err) => {
                    tracing::error!("command failed: {}", err);
                }
            }
            Ok(())
        });

        Ok(())
    }

    /// 获取服务端文件
    pub async fn get(&mut self, files: Vec<String>) -> HorseResult<()> {
        tracing::info!("GET: {}", files.join(" "));

        let env_repo = self.env.get("REPO").context("REPO 环境变量未设置")?;
        let _env_branch = self.env.get("BRANCH").context("BRANCH 环境变量未设置")?;

        let mut repo_path = PathBuf::from(env_repo);
        // 去除开头的 /
        if repo_path.starts_with("/") {
            repo_path.strip_prefix("/").context("REPO STRIP_PREFIX")?;
        }

        // 清理路径
        repo_path = repo_path.clean();

        // 裸仓库名称统一添加 .git 后缀
        if repo_path.extension() != Some(OsStr::new("git")) && !repo_path.set_extension("git") {
            tracing::error!("无效仓库路径: {:?}", repo_path);
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
        if repo_path.starts_with("/") {
            repo_path.strip_prefix("/").context("REPO STRIP_PREFIX")?;
        }

        // 清理路径
        repo_path = repo_path.clean();

        // 裸仓库名称统一添加 .git 后缀
        if repo_path.extension() != Some(OsStr::new("git")) && !repo_path.set_extension("git") {
            tracing::error!("无效仓库路径: {:?}", repo_path);
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
                return Ok(());
            }
        }

        let file_path = work_path.join(file_path);

        if !file_path.exists() {
            handle.error(format!("文件不存在: {}", file)).await?;
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

            Ok(())
        });

        Ok(())
    }

    /// ### 服务端 just 指令
    ///
    /// 用于持续集成的自动化任务, 往 just@xxx.xxx.xxx.xxx push 代码即可触发构建
    /// 目前主要用于跟 git 工作流配合
    ///
    pub async fn just(&mut self, command: Vec<String>) -> HorseResult<()> {
        tracing::info!("[just] {}", command.join(" "));
        let env_repo = self.env.get("REPO").context("REPO 环境变量未设置")?;
        let env_branch = self.env.get("BRANCH").context("BRANCH 环境变量未设置")?;

        let mut repo_path = PathBuf::from(env_repo);
        // 去除开头的 /
        if repo_path.starts_with("/") {
            repo_path.strip_prefix("/").context("REPO STRIP_PREFIX")?;
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
            return Ok(());
        }

        let repo = Repo::from(&repo_path);
        tracing::info!("GIT REPO: {}", repo.path().display());
        let task = self.tm.spawn_handle();

        // 1. 检出代码用于构建
        // 2. 执行项目的 just 命令, 项目必须包含 justfile 文件

        // 如果仓库目录不存在
        if !repo.exists() {
            handle.error("代码仓库不存在, 请先 push 代码").await?;
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

        if let Err(err) = repo
            .checkout(&work_path, Some(env_branch))
            .await
            .context("检出代码失败")
        {
            tracing::error!("{:?}", err);
            handle.error(err.to_string()).await?;
            return Ok(());
        }

        let t = task.clone();
        task.spawn(async move {
            let mut diff_input = handle.make_reader();
            let mut buf = vec![];
            diff_input.read_to_end(&mut buf).await?;

            repo.apply(&work_path, &buf).await.context("git apply")?;
            drop(diff_input);

            handle
                .info(format!("just {}...", command.join(" ")).bold().to_string())
                .await?;

            let mut cmd = Command::new("just");
            cmd.current_dir(&work_path);
            cmd.arg(command.join(" "));

            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());

            let mut cmd = cmd.spawn()?;

            let mut stdout = cmd.stdout.take().unwrap();
            let mut stderr = cmd.stderr.take().unwrap();

            let mut o_output = handle.make_writer();

            t.spawn(async move {
                while let Ok(len) = tokio::io::copy(&mut stdout, &mut o_output).await {
                    // eof
                    if len == 0 {
                        break;
                    }
                }
                Ok(())
            });

            const BUF_SIZE: usize = 1024 * 32;
            let mut buf = [0u8; BUF_SIZE];

            loop {
                let read = stderr.read(&mut buf).await?;
                if read == 0 {
                    break;
                }
                handle.log_raw(&buf[..read]).await?;
            }

            handle.info("构建完成").await?;
            handle.exit(cmd.wait().await?).await?;
            Ok(())
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

        if !repo.exists() {
            tracing::error!("仓库不存在: {}", repo.path().display());
            handle.error("仓库不存在").await?;
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
                return Ok(());
            }
        };

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;

            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        cmd.kill_on_drop(true);
        cmd.current_dir(&work_path);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let t2 = task.clone();
        task.spawn(async move {
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
            let mut cmd = cmd.spawn().context("cargo command failed")?;
            let mut stdout = cmd.stdout.take().unwrap();
            let mut stderr = cmd.stderr.take().unwrap();

            t2.spawn(async move {
                while let Ok(len) = tokio::io::copy(&mut stdout, &mut o_output).await {
                    // eof
                    if len == 0 {
                        break;
                    }
                }

                Ok(())
            });

            while let Ok(len) = tokio::io::copy(&mut stderr, &mut e_output).await {
                // eof
                if len == 0 {
                    break;
                }
            }

            handle.exit(cmd.wait().await?).await?;
            Ok(())
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
            let mut cmd = cmd.spawn()?;

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
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        tracing::info!("[{channel}] ssh env request: {key}={value}");
        self.env
            .insert(key.to_uppercase().to_string(), value.to_string());
        Ok(())
    }

    /// The client requests a shell.
    async fn shell_request(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
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
            "cargo" => self.cargo(command).await?,
            "apply" => self.apply(command).await?,
            // just 命令支持 just.xxx 格式, xxx 对应 justfile 中的运行指令
            "just" => self.just(command).await?,
            // action if action.starts_with("just") => {
            //     let mut subaction = action.split(".").skip(1).collect::<Vec<_>>().join(".");
            //     if subaction.is_empty() {
            //         subaction = "build".to_owned();
            //     }
            //     self.just(command, subaction).await?;
            // }
            "get" => self.get(command).await?,
            "scp" => self.scp(command).await?,
            action => {
                let handle = self.handle.take().context("FIXME: NO HANDLE").unwrap();
                handle.error(format!("不支持的命令: {action}")).await?;
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
    fn drop(&mut self) {
        tracing::info!("Drop AppServer");
    }
}

pub async fn run() -> HorseResult<()> {
    let mut server = AppServer::new();
    server.run().await.expect("Failed running server");
    Ok(())
}
