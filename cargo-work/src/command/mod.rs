#![allow(unused_variables)]
use crate::options::HorseOptions;
use color_eyre::eyre::{bail, ContextCompat, Result, WrapErr};
use colored::Colorize;
use git2::BranchType;
use git2::Remote;
use git2::Repository;
use russh::client::Msg;
use russh::client::{self, DisconnectReason, Handle, Handler};
use russh::keys::key::PrivateKeyWithHashAlg;
use russh::keys::ssh_key::PublicKey;
use russh::keys::*;
use russh::*;
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpSocket;
use tokio::net::ToSocketAddrs;
use url::Url;

pub mod admin;
pub mod cargo;
pub mod cmd;
pub mod get;
pub mod health;
pub mod job;
pub mod just;
pub mod logs;
pub mod ping;
pub mod pull;
pub mod push;
pub mod put;
pub mod scp;
pub mod ssh;
pub mod watch;

pub const TRACE_ID_ENV: &str = "HORSE_TRACE_ID";
pub const DEBUG_ENV: &str = "WH_DEBUG";
static TRACE_SEQ: AtomicU64 = AtomicU64::new(1);

pub struct HorseClient {
    handle: Handle<Client>,
}

pub struct Client {
    pub forward_host: Option<String>,
    pub forward_port: Option<u32>,
}

#[async_trait::async_trait]
impl Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(&mut self, _pk: &PublicKey) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn auth_banner(
        &mut self,
        banner: &str,
        _session: &mut russh::client::Session,
    ) -> Result<(), Self::Error> {
        for banner in banner.lines() {
            tracing::info!(
                "{}{}{} {}",
                "[".bold(),
                "HORSED".green(),
                "]".bold(),
                banner.yellow()
            );
        }
        Ok(())
    }

    /// Called when the server sent a disconnect message
    ///
    /// If reason is an Error, this function should re-return the error so the join can also evaluate it
    async fn disconnected(
        &mut self,
        reason: DisconnectReason<Self::Error>,
    ) -> Result<(), Self::Error> {
        match reason {
            DisconnectReason::ReceivedDisconnect(_) => Ok(()),
            DisconnectReason::Error(e) => Err(e),
        }
    }

    /// Called when the server sends us data. The `extended_code`
    /// parameter is a stream identifier, `None` is usually the
    /// standard output, and `Some(1)` is the standard error. See
    /// [RFC4254](https://tools.ietf.org/html/rfc4254#section-5.2).
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called when the server sends us data. The `extended_code`
    /// parameter is a stream identifier, `None` is usually the
    /// standard output, and `Some(1)` is the standard error. See
    /// [RFC4254](https://tools.ietf.org/html/rfc4254#section-5.2).
    async fn extended_data(
        &mut self,
        channel: ChannelId,
        ext: u32,
        data: &[u8],
        session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called when the server opens a channel for a new remote port forwarding connection
    #[allow(unused_variables)]
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        connected_address: &str,
        connected_port: u32,
        originator_address: &str,
        originator_port: u32,
        session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let host = self
            .forward_host
            .as_ref()
            .map_or(connected_address, |v| v.as_str());
        let port = self.forward_port.unwrap_or(connected_port);

        tracing::info!(
            "{}:{} <- {}:{} <- {}:{}",
            host,
            port,
            connected_address,
            connected_port,
            originator_address,
            originator_port
        );

        let socket = TcpSocket::new_v4()?;
        let Ok(mut stream) = socket
            .connect(format!("{}:{}", host, port).parse().unwrap())
            .await
        else {
            session.disconnect(Disconnect::ByApplication, "", "English")?;
            return Ok(());
        };

        tokio::spawn(async move {
            let mut ch_stream = channel.into_stream();
            tokio::io::copy_bidirectional(&mut ch_stream, &mut stream).await?;
            Ok::<_, Self::Error>(())
        });

        Ok(())
    }

    #[allow(unused_variables)]
    async fn channel_open_confirmation(
        &mut self,
        channel: ChannelId,
        max_packet_size: u32,
        window_size: u32,
        session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        // tracing::info!(
        //     "channel open: {:?} {} {}",
        //     channel, max_packet_size, window_size
        // );
        Ok(())
    }
}

impl HorseClient {
    #[allow(unused)]
    async fn connect<P: AsRef<Path>, A: ToSocketAddrs>(
        key_path: P,
        key_hash_alg: Option<HashAlg>,
        user: impl Into<String>,
        addrs: A,
        forward_host: Option<String>,
        forward_port: Option<u32>,
    ) -> Result<Self> {
        let key_pair = load_secret_key(key_path, None)?;
        let config = client::Config {
            inactivity_timeout: Some(Duration::from_secs(60)),
            keepalive_interval: Some(Duration::from_secs(3)),
            ..<_>::default()
        };

        let config = Arc::new(config);
        let sh = Client {
            forward_host,
            forward_port,
        };

        let mut handle = client::connect(config, addrs, sh).await?;
        let auth_res = handle
            .authenticate_publickey(
                user,
                PrivateKeyWithHashAlg::new(Arc::new(key_pair), key_hash_alg)?,
            )
            .await?;

        if !auth_res {
            bail!("Authentication failed");
        }

        Ok(Self { handle })
    }

    // Run interactive shell or other commands
    // The `command` will be attached a pseudo-terminal and executed on the server.
    async fn shell(&mut self, command: &str) -> Result<u32> {
        let mut channel = self.handle.channel_open_session().await?;
        let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
        // Request an interactive PTY from the server
        channel
            .request_pty(
                false,
                &std::env::var("TERM").unwrap_or("xterm".into()),
                cols as u32,
                rows as u32,
                0,
                0,
                &[], // ideally you want to pass the actual terminal modes here
            )
            .await?;
        channel.exec(true, command).await?;

        let mut code = 0;
        let mut stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut stderr = tokio::io::stderr();
        let mut buf = vec![0; 1024];
        let mut stdin_closed = false;

        loop {
            // Handle one of the possible events:
            tokio::select! {
                // There's terminal input available from the user
                r = stdin.read(&mut buf), if !stdin_closed => {
                    match r {
                        Ok(0) => {
                            stdin_closed = true;
                            channel.eof().await?;
                        },
                        // Send it to the server
                        Ok(n) => channel.data(&buf[..n]).await?,
                        Err(e) => return Err(e.into()),
                    };
                },
                // There's an event available on the session channel
                msg_opt = channel.wait() => {
                    match msg_opt {
                        Some(msg) => match msg {
                            // Write data to the terminal
                            ChannelMsg::Data { ref data } => {
                                stdout.write_all(data).await?;
                                stdout.flush().await?;
                            }
                            ChannelMsg::ExtendedData { ref data, .. } => {
                                stderr.write_all(data).await?;
                                stderr.flush().await?;
                            }
                            // The command has returned an exit code
                            ChannelMsg::ExitStatus { exit_status } => {
                                code = exit_status;
                                if !stdin_closed {
                                    channel.eof().await?;
                                }
                                break;
                            }
                            _ => {}
                        },
                        None => {
                            // Server closed the channel without sending an exit status (e.g., dropped connection)
                            tracing::warn!("Server closed channel unexpectedly");
                            if !stdin_closed {
                                channel.eof().await?;
                            }
                            break;
                        }
                    }
                },
            }
        }

        Ok(code)
    }

    #[allow(unused)]
    async fn close(&mut self) -> Result<()> {
        self.handle
            .disconnect(Disconnect::ByApplication, "", "English")
            .await?;
        Ok(())
    }
}

impl Deref for HorseClient {
    type Target = Handle<Client>;
    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

impl DerefMut for HorseClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.handle
    }
}

pub fn new_trace_id(action: &str) -> String {
    if !debug_enabled() {
        return String::new();
    }

    if let Ok(trace_id) = std::env::var(TRACE_ID_ENV) {
        if !trace_id.trim().is_empty() {
            return trace_id;
        }
    }

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let pid = std::process::id();
    let seq = TRACE_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{action}-{now_ms:x}-{pid:x}-{seq:x}")
}

pub fn debug_enabled() -> bool {
    matches!(
        std::env::var(DEBUG_ENV).ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("True")
    )
}

pub fn log_stage(trace_id: &str, action: &str, stage: &str) {
    if !debug_enabled() || trace_id.is_empty() {
        return;
    }
    tracing::info!(trace_id = %trace_id, action = action, stage = stage, "stage");
}

/// 获取默认的配置, 目前 `牛马` 设置远程仓库名为 `horsed` 或 `just-horsed`
fn find_remote<'a>(repo: &'a Repository, options: &'a HorseOptions) -> Option<Remote<'a>> {
    // --remote <REMOTE>
    if let Some(ref remote) = options.remote {
        return repo.find_remote(remote).ok();
    }

    if let Ok(remote) = repo.find_remote("horsed") {
        return Some(remote);
    }

    None
}

fn find_repo_name(options: &HorseOptions) -> Option<String> {
    // 如果参数中指定了远程仓库, 则使用参数指定的仓库
    if let Some(ref repo_name) = options.repo_name {
        return Some(repo_name.to_string());
    }

    // 如果参数中指定了远程仓库的 URL, 则使用参数指定的 URL
    if let Some(ref url) = options.repo {
        return extract_repo_name(url);
    }

    None
}

fn find_host(options: &HorseOptions) -> Option<SocketAddr> {
    options.repo.as_ref().and_then(|s| extract_host(s))
}

fn extract_host(url: &str) -> Option<SocketAddr> {
    let url = Url::parse(url).ok()?;

    url.socket_addrs(|| Some(2222))
        .ok()
        .and_then(|addrs| addrs.first().copied())
}

fn extract_repo_name(url: &str) -> Option<String> {
    let url = Url::parse(url).ok()?;
    url.path().strip_prefix("/").map(|s| s.to_string())
}

fn upstream_status(repo: &Repository) -> Result<Option<(String, usize)>> {
    let head = repo.head().wrap_err("读取 HEAD 失败")?;
    if !head.is_branch() {
        return Ok(None);
    }

    let Some(branch_name) = head.shorthand() else {
        return Ok(None);
    };

    let local = repo
        .find_branch(branch_name, BranchType::Local)
        .wrap_err_with(|| format!("读取本地分支 `{branch_name}` 失败"))?;

    let upstream = match local.upstream() {
        Ok(branch) => branch,
        Err(err) if err.code() == git2::ErrorCode::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };

    let local_commit = local.get().target().context("读取本地分支提交失败")?;
    let upstream_commit = upstream.get().target().context("读取上游分支提交失败")?;
    let upstream_name = upstream
        .get()
        .name()
        .context("读取上游分支名称失败")?
        .to_string();
    let (ahead, _) = repo
        .graph_ahead_behind(local_commit, upstream_commit)
        .wrap_err("计算 ahead/behind 失败")?;

    Ok(Some((upstream_name, ahead)))
}

async fn git_diff(repo: &Repository, args: &[&str]) -> Result<Vec<u8>> {
    let mut cmd = tokio::process::Command::new("git");
    #[cfg(target_os = "windows")]
    {
        #[allow(unused_imports)]
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd.current_dir(repo.workdir().unwrap_or_else(|| std::path::Path::new(".")));
    cmd.stdout(std::process::Stdio::piped());
    cmd.arg("diff");
    cmd.args(args);

    let mut child = cmd.spawn().wrap_err("启动 git diff 失败")?;
    let mut out = vec![];
    child
        .stdout
        .take()
        .context("读取 git diff stdout 失败")?
        .read_to_end(&mut out)
        .await?;
    let status = child.wait().await?;
    if !status.success() {
        bail!(
            "git diff {} 失败 (exit={})",
            args.join(" "),
            status.code().unwrap_or(128)
        );
    }
    Ok(out)
}

pub async fn collect_remote_patch(repo: &Repository) -> Result<Vec<u8>> {
    let mut patch = vec![];

    if let Some((upstream, ahead)) = upstream_status(repo)? {
        if ahead > 0 {
            tracing::warn!(
                "检测到本地分支领先上游 {ahead} 个提交，将同步未推送提交到远端工作区: {upstream}"
            );
            let range = format!("{upstream}..HEAD");
            let commit_patch = git_diff(repo, &["--binary", range.as_str()]).await?;
            if commit_patch.is_empty() {
                bail!("本地分支领先上游 {ahead} 个提交，但未能生成补丁，请先 push 后重试");
            }
            patch.extend_from_slice(&commit_patch);
        }
    }

    // Includes both staged and unstaged local changes.
    let worktree_patch = git_diff(repo, &["--binary", "HEAD"]).await?;
    patch.extend_from_slice(&worktree_patch);

    Ok(patch)
}

#[cfg(feature = "use-system-ssh")]
fn run_system_ssh<K, V, Envs, Arg, Args>(
    key: &Path,
    env: Envs,
    action: &str,
    host: SocketAddr,
    args: Args,
) -> tokio::process::Command
where
    K: AsRef<str>,
    V: AsRef<str>,
    Envs: IntoIterator<Item = (K, V)>,
    Arg: AsRef<std::ffi::OsStr>,
    Args: std::iter::IntoIterator<Item = Arg>,
{
    let mut cmd = tokio::process::Command::new("ssh");

    #[cfg(target_os = "windows")]
    {
        #[allow(unused_imports)]
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd.arg("-i");
    cmd.arg(key);
    cmd.arg("-o");
    cmd.arg(format!(
        "SetEnv {}",
        env.into_iter()
            .map(|(k, v)| format!("{}={}", k.as_ref(), v.as_ref()))
            .collect::<Vec<_>>()
            .join(" ")
    ));
    cmd.arg(format!("{}@{}", action, host.ip()));
    cmd.arg("-p");
    cmd.arg(format!("{}", host.port()));
    cmd.args(args);

    cmd
}

#[cfg(test)]
mod tests {
    use url::Url;

    #[test]
    fn test_url_parse() {
        Url::parse("ssh://git@127.0.0.1:2222").ok().unwrap();
        Url::parse("http://127.0.0.1:1234").ok().unwrap();
        Url::parse("socks://127.0.0.1:1234").ok().unwrap();
    }
}
