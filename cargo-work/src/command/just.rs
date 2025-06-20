use super::*;
use crate::options::JustOptions;
use color_eyre::eyre::{anyhow, ContextCompat, Result};
use git2::Repository;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn run(sk: &Path, options: JustOptions) -> Result<()> {
    let repo = Repository::discover(".")?;
    let head = repo.head()?;

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

    let branch = head
        .shorthand()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "master".to_owned());
    let mut command = options.command.join(" "); //.unwrap_or("default".to_string());
    if command.is_empty() {
        command = "default".to_string();
    }

    let env = super::ssh::start_proxy(sk, host, &options.horse).await?;

    // git diff HEAD
    let mut cmd = tokio::process::Command::new("git");
    #[cfg(target_os = "windows")]
    {
        #[allow(unused_imports)]
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.arg("diff").arg("HEAD");

    let mut cmd = cmd.spawn()?;

    let mut diff = vec![];
    cmd.stdout.take().unwrap().read_to_end(&mut diff).await?;
    cmd.wait().await?;
    // git diff HEAD

    #[cfg(feature = "use-system-ssh")]
    {
        // ssh just@horsed <ACTION>
        use std::collections::HashMap;
        let mut envs = HashMap::new();

        let head_commit = head.peel_to_commit()?;
        let commit = head_commit.id().to_string();
        let message = head_commit.message();

        envs.insert("REPO".to_string(), repo_name);
        envs.insert("BRANCH".to_string(), branch);

        envs.insert("GIT_COMMIT".to_string(), commit);
        if let Some(message) = message {
            envs.insert("GIT_MESSAGE".to_string(), message.to_string());
        }

        for kv in env.iter() {
            let (k, v) = kv.split_once('=').unwrap_or_else(|| (kv, ""));
            envs.insert(k.to_string(), v.to_string());
        }

        if let Some(justfile) = options.file {
            envs.insert("JUSTFILE".to_string(), justfile);
        }

        let mut cmd =
            super::run_system_ssh(sk, envs, "just", host, [std::ffi::OsString::from(command)]);

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut ssh = cmd.spawn()?;
        let mut sshin = ssh.stdin.take().unwrap();

        sshin.write_all(&diff).await?;
        sshin.shutdown().await?;
        drop(sshin);

        let mut sshout = ssh.stdout.take().unwrap();
        let mut ssherr = ssh.stderr.take().unwrap();
        let mut stdout = tokio::io::stdout();
        let mut stderr = tokio::io::stderr();

        let write_out = tokio::io::copy(&mut sshout, &mut stdout);
        let write_err = tokio::io::copy(&mut ssherr, &mut stderr);

        futures::future::try_join(write_out, write_err).await?;

        ssh.wait().await?;
    }

    #[cfg(not(feature = "use-system-ssh"))]
    {
        let mut ssh =
            HorseClient::connect(sk, options.horse.key_hash_alg, "just", host, None, None).await?;
        let mut channel = ssh.channel_open_session().await?;
        let head_commit = head.peel_to_commit()?;
        let commit = head_commit.id().to_string();
        let message = head_commit.message();

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
        if let Some(justfile) = options.file {
            channel.set_env(true, "JUSTFILE", justfile).await?;
        }
        channel.exec(true, command.as_bytes()).await?;

        let mut stdin = channel.make_writer();
        stdin.write_all(&diff).await?;
        stdin.shutdown().await?;
        drop(stdin);

        let mut stdout = tokio::io::stdout();
        let mut stderr = tokio::io::stderr();

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
                ChannelMsg::Close => {}
                ChannelMsg::Eof => {}
                ChannelMsg::ExitStatus { exit_status } => {}
                other => {}
            }
        }

        ssh.close().await?;
    }

    Ok(())
}
