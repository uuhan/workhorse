use super::*;
use crate::options::CargoKind;
use crate::options::Test;
use anyhow::Context;
use anyhow::Result;
use git2::Repository;
use std::path::Path;

pub async fn run(sk: &Path, options: impl CargoKind) -> Result<()> {
    let repo = Repository::discover(".")?;
    let head = repo.head()?;

    let repo_name = if let Some(repo_name) = find_repo_name(options.horse_options()) {
        repo_name
    } else {
        // 无法从参数获取 repo_name, 尝试从 git remote 获取
        // 默认远程仓库为 horsed,
        // 格式: ssh://git@192.168.10.62:2222/<ns>/<repo_name>
        let Some(horsed) = find_remote(&repo) else {
            return Err(anyhow::anyhow!("找不到 horsed 远程仓库!"));
        };

        horsed
            .url()
            .and_then(extract_repo_name)
            .context("获取 horsed 远程仓库 URL 失败")?
    };

    let host = if let Ok(host) = std::env::var("HORSED") {
        host.parse()
            .context(format!("解析环境变量 HORSED 失败: {host}"))?
    } else if let Some(host) = find_host(options.horse_options()) {
        host
    } else {
        let Some(horsed) = find_remote(&repo) else {
            return Err(anyhow::anyhow!("找不到 horsed 远程仓库!"));
        };

        horsed
            .url()
            .and_then(extract_host)
            .context("获取 horsed 远程仓库 HOST 失败")?
    };

    let branch = head
        .shorthand()
        .map(|s| s.to_string())
        // 默认分支为 master
        .unwrap_or_else(|| "master".to_owned());

    #[cfg(feature = "use-system-ssh")]
    {
        let mut cmd = super::run_system_ssh(
            sk,
            format!(
                "SetEnv REPO={} BRANCH={} CARGO_OPTIONS=\'{}\'",
                repo_name,
                branch,
                serde_json::to_string(options.cargo_options())?
            ),
            "cargo",
            host,
            options.name(),
        );
        let mut ssh = cmd.spawn()?;
        let mut stdout = ssh.stdout.take().unwrap();
        let mut out = tokio::io::stdout();

        while let Ok(len) = tokio::io::copy(&mut stdout, &mut out).await {
            if len == 0 {
                break;
            }
        }
    }

    #[cfg(not(feature = "use-system-ssh"))]
    {
        use russh::ChannelMsg;
        use tokio::io::AsyncWriteExt;
        let mut ssh = HorseClient::connect(sk, "cargo", host).await?;

        let mut channel = ssh.channel_open_session().await?;
        channel.set_env(true, "REPO", repo_name).await?;
        channel.set_env(true, "BRANCH", branch).await?;
        channel
            .set_env(
                true,
                "CARGO_OPTIONS",
                serde_json::to_string(options.cargo_options())?,
            )
            .await?;
        channel.exec(true, options.name()).await?;

        let mut code = None;
        let mut stdout = tokio::io::stdout();

        loop {
            // There's an event available on the session channel
            let Some(msg) = channel.wait().await else {
                break;
            };
            match msg {
                ChannelMsg::Success => {}

                // Write data to the terminal
                ChannelMsg::Data { ref data } => {
                    stdout.write_all(data).await?;
                    stdout.flush().await?;
                }
                // The command has returned an exit code
                ChannelMsg::ExitStatus { exit_status } => {
                    code = Some(exit_status);
                }

                ChannelMsg::ExtendedData { ref data, ext } => {
                    stdout.write_all(data).await?;
                    stdout.flush().await?;
                }

                ChannelMsg::Eof => {
                    break;
                }
                e => {}
            }
        }

        ssh.close().await?;
        code.context("program did not exit cleanly")?;
    }

    Ok(())
}
