use super::*;
use crate::options::ScpOptions;
use color_eyre::eyre::{anyhow, ContextCompat, Result};
use git2::Repository;
use std::path::Path;

pub async fn run(sk: &Path, options: ScpOptions) -> Result<()> {
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

    #[cfg(not(feature = "use-system-ssh"))]
    let mut channel = {
        use color_eyre::eyre::WrapErr;
        let ssh = HorseClient::connect(sk, "scp", host, None, None).await?;
        let channel = ssh.channel_open_session().await?;
        channel.set_env(true, "REPO", repo_name).await?;
        channel.set_env(true, "BRANCH", branch).await?;

        channel
            .exec(true, options.source.as_bytes())
            .await
            .wrap_err("exec")?;

        channel
    };
    #[cfg(not(feature = "use-system-ssh"))]
    let mut stdout = channel.make_reader();

    #[cfg(feature = "use-system-ssh")]
    let mut ssh = {
        let mut cmd = super::run_system_ssh(
            sk,
            &[("REPO", repo_name), ("BRANCH", branch)],
            "scp",
            host,
            [std::ffi::OsString::from(&options.source)],
        );
        cmd.kill_on_drop(true);
        cmd.stdout(std::process::Stdio::piped());

        cmd.spawn()?
    };
    #[cfg(feature = "use-system-ssh")]
    let mut stdout = ssh.stdout.take().unwrap();

    let mut file = tokio::fs::File::create_new(&options.dest).await?;

    while let Ok(len) = tokio::io::copy(&mut stdout, &mut file).await {
        if len == 0 {
            break;
        }
    }

    Ok(())
}
