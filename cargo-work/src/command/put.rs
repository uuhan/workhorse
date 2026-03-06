use super::*;
use crate::options::PutOptions;
use clean_path::Clean;
use color_eyre::eyre::{anyhow, bail, ContextCompat, Result, WrapErr};
use git2::Repository;
use std::io::{IsTerminal, Write};
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};

struct Osc94Progress {
    enabled: bool,
    total: u64,
    last_percent: Option<u8>,
}

impl Osc94Progress {
    fn new(total: u64) -> Self {
        Self {
            enabled: std::io::stderr().is_terminal(),
            total,
            last_percent: None,
        }
    }

    fn percent(&self, current: u64) -> u8 {
        if self.total == 0 {
            return 100;
        }

        ((current.saturating_mul(100) / self.total).min(100)) as u8
    }

    fn emit(&self, state: u8, progress: u8) {
        if !self.enabled {
            return;
        }

        eprint!("\x1b]9;4;{};{}\x07", state, progress);
        let _ = std::io::stderr().flush();
    }

    fn start(&mut self) {
        self.last_percent = Some(0);
        self.emit(1, 0);
    }

    fn update(&mut self, current: u64) {
        let percent = self.percent(current);
        if self.last_percent == Some(percent) {
            return;
        }

        self.last_percent = Some(percent);
        self.emit(1, percent);
    }

    fn finish_success(&mut self) {
        self.emit(1, 100);
        self.emit(0, 0);
    }

    fn finish_error(&mut self, current: u64) {
        self.emit(2, self.percent(current));
    }
}

async fn upload_with_osc94<W: AsyncWrite + Unpin>(
    local_path: &Path,
    total_size: u64,
    remote_stdin: &mut W,
) -> Result<()> {
    const BUF_SIZE: usize = 1024 * 64;
    let mut progress = Osc94Progress::new(total_size);
    let mut local_file = tokio::fs::File::open(local_path).await?;
    let mut sent: u64 = 0;
    let mut buf = vec![0_u8; BUF_SIZE];

    progress.start();
    let transfer_res: Result<()> = async {
        loop {
            let len = local_file.read(&mut buf).await?;
            if len == 0 {
                break;
            }

            remote_stdin.write_all(&buf[..len]).await?;
            sent += len as u64;
            progress.update(sent);
        }

        remote_stdin.flush().await?;
        Ok(())
    }
    .await;

    if transfer_res.is_ok() {
        progress.finish_success();
    } else {
        progress.finish_error(sent);
    }

    transfer_res
}

pub async fn run(sk: &Path, options: PutOptions) -> Result<()> {
    let action = "put";
    let trace_id = super::new_trace_id(action);
    super::log_stage(&trace_id, action, "resolve.start");
    let repo = Repository::discover(".")?;
    let head = repo.head()?;

    let local = options.local.clean();
    let local_md = std::fs::metadata(&local)
        .wrap_err_with(|| format!("读取本地文件失败: {}", local.display()))?;
    if !local_md.is_file() {
        return Err(anyhow!("本地路径不是文件: {}", local.display()));
    }

    let repo_name = if let Some(repo_name) = find_repo_name(&options.horse) {
        repo_name
    } else {
        // 无法从参数获取 repo_name, 尝试从 git remote 获取
        // 默认远程仓库为 horsed,
        // 格式: ssh://git@192.168.10.62:2222/<ns>/<repo_name>
        let Some(horsed) = find_remote(&repo, &options.horse) else {
            return Err(anyhow!("找不到 horsed 远程仓库!"));
        };

        horsed
            .url()
            .and_then(extract_repo_name)
            .context("获取 horsed 远程仓库 URL 失败")?
    };

    let host = if let Ok(host) = std::env::var("HORSED") {
        host.parse()?
    } else if let Some(host) = find_host(&options.horse) {
        host
    } else {
        let Some(horsed) = find_remote(&repo, &options.horse) else {
            return Err(anyhow!("找不到 horsed 远程仓库!"));
        };

        horsed
            .url()
            .and_then(extract_host)
            .context("获取 horsed 远程仓库 HOST 失败")?
    };
    super::log_stage(&trace_id, action, "resolve.done");

    let branch = head
        .shorthand()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "master".to_owned());

    #[cfg(not(feature = "use-system-ssh"))]
    {
        super::log_stage(&trace_id, action, "connect.start");
        let mut ssh =
            HorseClient::connect(sk, options.horse.key_hash_alg, "put", host, None, None).await?;
        let mut channel = ssh.channel_open_session().await?;

        if !trace_id.is_empty() {
            channel
                .set_env(true, super::TRACE_ID_ENV, &trace_id)
                .await?;
        }
        channel.set_env(true, "REPO", repo_name).await?;
        channel.set_env(true, "BRANCH", branch).await?;
        for kv in options.horse.env.iter() {
            let (k, v) = kv.split_once('=').unwrap_or_else(|| (kv, ""));
            channel.set_env(true, k, v).await?;
        }

        super::log_stage(&trace_id, action, "dispatch.exec");
        channel
            .exec(
                true,
                shell_escape::escape(options.dest.clone().into()).to_string(),
            )
            .await
            .wrap_err("exec")?;

        let mut stdin = channel.make_writer();
        upload_with_osc94(&local, local_md.len(), &mut stdin).await?;
        stdin.shutdown().await?;
        drop(stdin);

        let mut stdout = tokio::io::stdout();
        let mut stderr = tokio::io::stderr();
        let mut code = 0_u32;
        let mut got_exit_status = false;

        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { ref data } => {
                    stdout.write_all(data).await?;
                    stdout.flush().await?;
                }
                ChannelMsg::ExtendedData { ref data, .. } => {
                    stderr.write_all(data).await?;
                    stderr.flush().await?;
                }
                ChannelMsg::ExitStatus { exit_status } => {
                    got_exit_status = true;
                    code = exit_status;
                    if super::debug_enabled() && !trace_id.is_empty() {
                        tracing::info!(
                            trace_id = %trace_id,
                            action = action,
                            stage = "remote.exit",
                            exit_status
                        );
                    }
                }
                ChannelMsg::Close | ChannelMsg::Eof => {}
                _ => {}
            }
        }

        if !ssh.is_closed() {
            ssh.close().await?;
        }

        if got_exit_status && code != 0 {
            bail!("remote put failed with exit status {code}");
        }
        super::log_stage(&trace_id, action, "done");
    }

    #[cfg(feature = "use-system-ssh")]
    {
        use std::collections::HashMap;
        let mut envs = HashMap::new();
        if !trace_id.is_empty() {
            envs.insert(super::TRACE_ID_ENV.to_string(), trace_id.clone());
        }
        envs.insert("REPO".to_string(), repo_name);
        envs.insert("BRANCH".to_string(), branch);
        for kv in options.horse.env.iter() {
            let (k, v) = kv.split_once('=').unwrap_or_else(|| (kv, ""));
            envs.insert(k.to_string(), v.to_string());
        }

        super::log_stage(&trace_id, action, "connect.start");
        let mut cmd = super::run_system_ssh(
            sk,
            envs,
            "put",
            host,
            [std::ffi::OsString::from(options.dest)],
        );
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut ssh = cmd.spawn()?;
        let mut sshin = ssh.stdin.take().unwrap();
        let mut sshout = ssh.stdout.take().unwrap();
        let mut ssherr = ssh.stderr.take().unwrap();
        upload_with_osc94(&local, local_md.len(), &mut sshin).await?;
        sshin.shutdown().await?;
        drop(sshin);

        let mut out = tokio::io::stdout();
        let mut err = tokio::io::stderr();
        let write_out = tokio::io::copy(&mut sshout, &mut out);
        let write_err = tokio::io::copy(&mut ssherr, &mut err);
        futures::future::try_join(write_out, write_err).await?;

        let status = ssh.wait().await?;
        if !status.success() {
            bail!(
                "remote put failed with exit status {}",
                status.code().unwrap_or(128)
            );
        }
        super::log_stage(&trace_id, action, "done");
    }

    Ok(())
}
