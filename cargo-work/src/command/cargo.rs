use super::*;
use crate::options::CargoKind;
use color_eyre::eyre::{anyhow, bail, ContextCompat, Result, WrapErr};
use git2::Repository;
use std::path::Path;
use tokio::io::AsyncWriteExt;

pub async fn run(sk: &Path, options: impl CargoKind) -> Result<()> {
    let action = "cargo";
    let trace_id = super::new_trace_id(action);
    super::log_stage(&trace_id, action, "resolve.start");
    let repo = Repository::discover(".")?;
    let head = repo.head()?;

    let repo_name = if let Some(repo_name) = find_repo_name(options.horse_options()) {
        repo_name
    } else {
        // 无法从参数获取 repo_name, 尝试从 git remote 获取
        // 默认远程仓库为 horsed,
        // 格式: ssh://git@192.168.10.62:2222/<ns>/<repo_name>
        let Some(horsed) = find_remote(&repo, options.horse_options()) else {
            return Err(anyhow!("找不到 horsed 远程仓库!"));
        };

        horsed
            .url()
            .and_then(extract_repo_name)
            .wrap_err("获取 horsed 远程仓库 URL 失败")?
    };

    let host = if let Ok(host) = std::env::var("HORSED") {
        host.parse()?
    } else if let Some(host) = find_host(options.horse_options()) {
        host
    } else {
        let Some(horsed) = find_remote(&repo, options.horse_options()) else {
            return Err(anyhow!("找不到 horsed 远程仓库!"));
        };

        horsed
            .url()
            .and_then(extract_host)
            .wrap_err("获取 horsed 远程仓库 HOST 失败")?
    };
    super::log_stage(&trace_id, action, "resolve.done");

    let branch = head
        .shorthand()
        .map(|s| s.to_string())
        // 默认分支为 master
        .unwrap_or_else(|| "master".to_owned());

    let env = super::ssh::start_proxy(sk, host, options.horse_options()).await?;
    super::log_stage(&trace_id, action, "proxy.ready");

    let diff =
        super::collect_remote_patch(&repo, options.horse_options().remote.as_deref()).await?;

    #[cfg(not(feature = "use-system-ssh"))]
    {
        super::log_stage(&trace_id, action, "connect.start");
        let mut ssh = HorseClient::connect(
            sk,
            options.horse_options().key_hash_alg,
            "cargo",
            host,
            None,
            None,
        )
        .await?;
        let mut channel = ssh.channel_open_session().await?;
        super::log_stage(&trace_id, action, "channel.open");
        let head_commit = head.peel_to_commit()?;
        let commit = head_commit.id().to_string();
        let message = head_commit.message();

        if !trace_id.is_empty() {
            channel
                .set_env(true, super::TRACE_ID_ENV, &trace_id)
                .await?;
        }
        channel.set_env(true, "REPO", repo_name).await?;
        channel.set_env(true, "BRANCH", branch).await?;
        channel.set_env(true, "GIT_COMMIT", commit).await?;
        if let Some(message) = message {
            channel.set_env(true, "GIT_MESSAGE", message).await?;
        }

        for kv in env.iter() {
            let (k, v) = kv.split_once('=').unwrap_or_else(|| (kv, ""));
            channel.set_env(true, k, v).await?;
        }

        channel
            .set_env(true, "ZIGBUILD", options.use_zigbuild().to_string())
            .await?;
        channel
            .set_env(
                true,
                "CARGO_OPTIONS",
                serde_json::to_string(options.cargo_options())?,
            )
            .await?;

        super::log_stage(&trace_id, action, "dispatch.exec");
        channel.exec(true, options.name()).await.wrap_err("exec")?;

        let mut writer = channel.make_writer();
        writer.write_all(&diff).await.unwrap();
        writer.shutdown().await?;

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
            bail!("remote cargo command failed with exit status {code}");
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
        envs.insert("ZIGBUILD".to_string(), options.use_zigbuild().to_string());
        envs.insert(
            "CARGO_OPTIONS".to_string(),
            format!("\'{}\'", serde_json::to_string(options.cargo_options())?),
        );

        let head_commit = head.peel_to_commit()?;
        let commit = head_commit.id().to_string();
        let message = head_commit.message();

        envs.insert("GIT_COMMIT".to_string(), commit);
        if let Some(message) = message {
            envs.insert("GIT_MESSAGE".to_string(), message.to_string());
        }

        for kv in env.iter() {
            let (k, v) = kv.split_once('=').unwrap_or_else(|| (kv, ""));
            envs.insert(k.to_string(), v.to_string());
        }

        // ssh cargo@horsed build --
        let mut args = vec![std::ffi::OsString::from(options.name())];
        args.extend(options.options().into_iter());

        let mut cmd = super::run_system_ssh(sk, envs, "cargo", host, args);

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.stdin(std::process::Stdio::piped());
        let mut ssh = cmd.spawn().wrap_err("ssh")?;
        let mut stdout = ssh.stdout.take().unwrap();
        let mut stderr = ssh.stderr.take().unwrap();
        let mut stdin = ssh.stdin.take().unwrap();
        let mut out = tokio::io::stdout();
        let mut err = tokio::io::stderr();

        stdin.write_all(&diff).await?;
        drop(stdin);

        let write_out = tokio::io::copy(&mut stdout, &mut out);
        let write_err = tokio::io::copy(&mut stderr, &mut err);
        futures::future::try_join(write_out, write_err).await?;

        let status = ssh.wait().await?;
        if !status.success() {
            bail!(
                "remote cargo command failed with exit status {}",
                status.code().unwrap_or(128)
            );
        }
        super::log_stage(&trace_id, action, "done");
    }

    Ok(())
}
