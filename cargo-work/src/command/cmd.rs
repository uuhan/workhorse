use super::*;
use color_eyre::eyre::{anyhow, ContextCompat, Result};
use git2::Repository;
use std::path::Path;

pub async fn run(sk: &Path, horse: HorseOptions, scripts: Vec<String>) -> Result<()> {
    let repo = Repository::discover(".")?;
    let head = repo.head()?;

    let repo_name = if let Some(repo_name) = find_repo_name(&horse) {
        repo_name
    } else {
        // 无法从参数获取 repo_name, 尝试从 git remote 获取
        // 默认远程仓库为 horsed,
        // 格式: ssh://git@192.168.10.62:2222/<ns>/<repo_name>
        let Some(horsed) = find_remote(&repo, &horse) else {
            return Err(anyhow!("找不到 horsed 远程仓库!"));
        };

        horsed
            .url()
            .and_then(extract_repo_name)
            .context("获取 horsed 远程仓库 URL 失败")?
    };

    let host = if let Ok(host) = std::env::var("HORSED") {
        host.parse()?
    } else if let Some(host) = find_host(&horse) {
        host
    } else {
        let Some(horsed) = find_remote(&repo, &horse) else {
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

    let env = super::ssh::start_proxy(sk, host, &horse).await?;

    #[cfg(not(feature = "use-system-ssh"))]
    let mut channel = {
        use color_eyre::eyre::WrapErr;
        let ssh = HorseClient::connect(sk, horse.key_hash_alg, "cmd", host, None, None).await?;
        let channel = ssh.channel_open_session().await?;

        if let Some(shell) = horse.shell {
            channel.set_env(true, "SHELL", shell).await?;
        }

        if horse.pty {
            channel.set_env(true, "PTY", "1").await?;
        }

        channel.set_env(true, "REPO", repo_name).await?;
        channel.set_env(true, "BRANCH", branch).await?;
        for kv in env.iter() {
            let (k, v) = kv.split_once('=').unwrap_or_else(|| (kv, ""));
            channel.set_env(true, k, v).await?;
        }

        channel
            .exec(true, scripts.join(" ").as_bytes())
            .await
            .wrap_err("exec")?;

        channel
    };
    #[cfg(not(feature = "use-system-ssh"))]
    let mut stdout = channel.make_reader();

    #[cfg(feature = "use-system-ssh")]
    let mut ssh = {
        use std::collections::HashMap;
        let mut envs = HashMap::new();
        envs.insert("REPO".to_string(), repo_name);
        envs.insert("BRANCH".to_string(), branch);
        if let Some(shell) = horse.shell {
            envs.insert("SHELL".to_string(), shell);
        }

        for kv in env.iter() {
            let (k, v) = kv.split_once('=').unwrap_or_else(|| (kv, ""));
            envs.insert(k.to_string(), v.to_string());
        }

        let mut cmd = super::run_system_ssh(sk, envs, "cmd", host, scripts);
        cmd.stdout(std::process::Stdio::piped());
        cmd.spawn()?
    };
    #[cfg(feature = "use-system-ssh")]
    let mut stdout = ssh.stdout.take().unwrap();

    let mut cout = tokio::io::stdout();

    while let Ok(len) = tokio::io::copy(&mut stdout, &mut cout).await {
        if len == 0 {
            break;
        }
    }

    Ok(())
}
