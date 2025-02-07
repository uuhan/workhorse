use super::*;
use crate::options::JustOptions;
use color_eyre::eyre::{anyhow, ContextCompat, Result};
use git2::Repository;
use std::path::Path;
use tokio::io::AsyncWriteExt;

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
    let command = options.command.unwrap_or("default".to_string());

    #[cfg(feature = "use-system-ssh")]
    {
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
        use tokio::io::AsyncReadExt;
        cmd.stdout.take().unwrap().read_to_end(&mut diff).await?;
        cmd.wait().await?;
        // git diff HEAD

        // ssh just@horsed <ACTION>
        let mut cmd = super::run_system_ssh(
            sk,
            &[("REPO", repo_name), ("BRANCH", branch)],
            "just",
            host,
            [std::ffi::OsString::from(command)],
        );
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        let mut ssh = cmd.spawn()?;
        let mut stdin = ssh.stdin.take().unwrap();
        let mut stdout = ssh.stdout.take().unwrap();
        let mut out = tokio::io::stdout();

        stdin.write_all(&diff).await?;
        drop(stdin);

        while let Ok(len) = tokio::io::copy(&mut stdout, &mut out).await {
            if len == 0 {
                break;
            }
        }
        ssh.wait().await?;
    }

    #[cfg(not(feature = "use-system-ssh"))]
    {
        let mut ssh = HorseClient::connect(sk, "just", host).await?;
        let mut channel = ssh.channel_open_session().await?;
        channel.set_env(true, "REPO", repo_name).await?;
        channel.set_env(true, "BRANCH", branch).await?;
        channel.exec(true, command.as_bytes()).await?;

        let mut stdout = tokio::io::stdout();
        let mut stderr = tokio::io::stderr();

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
                ChannelMsg::ExitStatus { exit_status } => {}

                ChannelMsg::ExtendedData { ref data, ext } => {
                    stderr.write_all(data).await?;
                    stderr.flush().await?;
                }

                ChannelMsg::Eof => {
                    break;
                }
                e => {}
            }
        }

        ssh.close().await?;
    }

    Ok(())
}
