use super::*;
use crate::options::PingOptions;
use color_eyre::eyre::WrapErr;
use color_eyre::eyre::{anyhow, ContextCompat, Result};
use git2::Repository;
use stable::data::v2::{self, Body};
use stable::data::{Head, HEAD_SIZE};
use std::path::Path;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use zerocopy::{FromBytes, IntoBytes};

pub async fn run(sk: &Path, options: PingOptions) -> Result<()> {
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

    // #[cfg(feature = "use-system-ssh")]
    // {
    //     let mut cmd = super::run_system_ssh::<&str, &str, &str, _>(sk, &[], "ping", host, []);
    //     cmd.stdin(std::process::Stdio::piped());
    //     cmd.stdout(std::process::Stdio::piped());
    //     let mut ssh = cmd.spawn()?;
    //
    //     let mut sshin = ssh.stdin.take().unwrap();
    //     let mut sshout = ssh.stdout.take().unwrap();
    //
    //     let mut cout = tokio::io::stdout();
    //
    //     while let Ok(len) = tokio::io::copy(&mut sshout, &mut cout).await {
    //         if len == 0 {
    //             break;
    //         }
    //     }
    // }

    let mut ssh = HorseClient::connect(sk, "ping", host).await?;
    let mut channel = ssh.channel_open_session().await?;

    channel.exec(true, &[]).await.wrap_err("ssh exec")?;

    let mut sshin = channel.make_writer();
    let mut sshout = channel.make_reader();

    let ping = bincode::serialize(&Body::Ping(Instant::now()))?;
    let head = v2::head(ping.len() as _);

    sshin.write_all(head.as_bytes()).await?;
    sshin.write_all(&ping).await?;

    let mut head = [0u8; HEAD_SIZE];
    sshout.read_exact(&mut head).await?;
    let head = Head::ref_from_bytes(&head).unwrap();

    let mut body = vec![0u8; head.size as usize];
    sshout.read_exact(&mut body).await?;

    let body = bincode::deserialize::<Body>(&body)?;
    match body {
        Body::Pong(instant) => {
            println!("ping: {:?}", instant.elapsed());
        }
        _ => {
            return Err(anyhow!("ping 失败!"));
        }
    }

    ssh.close().await?;
    Ok(())
}
