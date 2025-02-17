use super::*;
use crate::options::SshOptions;
use color_eyre::eyre::{anyhow, ContextCompat, Result, WrapErr};
use git2::Repository;
use std::path::Path;
use tokio::net::TcpListener;

pub async fn run(sk: &Path, options: SshOptions) -> Result<()> {
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

    // ssh -L
    if let Some(forward_local_port) = options.forward_local_port {
        let mut ssh = HorseClient::connect(sk, "ssh", host, None, None).await?;

        let mut addrs = forward_local_port.split(":").collect::<Vec<&str>>();
        addrs.reverse();
        let remote_port = addrs.first().unwrap().parse::<u32>()?;
        let remote_host = addrs.get(1).unwrap().parse::<String>()?;
        let local_port = addrs.get(2).unwrap().parse::<u32>()?;
        let local_host = addrs
            .get(3)
            .and_then(|s| s.parse::<String>().ok())
            .unwrap_or("127.0.0.1".to_string());

        let local_addr = format!("{}:{}", local_host, local_port);
        println!("Listening on {}", local_addr);
        let listener = TcpListener::bind(&local_addr).await?;

        while let Ok((mut stream, addr)) = listener.accept().await {
            println!(
                "{:?} -> {} -> {}:{}",
                addr, local_addr, remote_host, remote_port
            );

            let channel = match ssh
                .channel_open_direct_tcpip(&remote_host, remote_port, &local_host, local_port)
                .await
                .wrap_err("tcp forward failed!")
            {
                Ok(channel) => channel,
                Err(e) => {
                    eprintln!("tcpip forward failed: {:?}", e);
                    ssh.close().await?;
                    return Ok(());
                }
            };

            let mut ch_stream = channel.into_stream();
            tokio::io::copy_bidirectional(&mut ch_stream, &mut stream).await?;
        }
    }

    // ssh -R
    if let Some(forward_remote_port) = options.forward_remote_port {
        let mut addrs = forward_remote_port.split(":").collect::<Vec<&str>>();
        addrs.reverse();
        let local_port = addrs
            .first()
            .unwrap()
            .parse::<u32>()
            .context("port parse")?;
        let local_host = addrs
            .get(1)
            .unwrap()
            .parse::<String>()
            .context("host parse")?;
        let remote_port = addrs.get(2).unwrap().parse::<u32>().context("port parse")?;
        let remote_host = addrs
            .get(3)
            .and_then(|s| s.parse::<String>().ok())
            .unwrap_or("127.0.0.1".to_string());

        let mut ssh =
            HorseClient::connect(sk, "ssh", host, Some(local_host), Some(local_port)).await?;

        ssh.tcpip_forward(&remote_host, remote_port).await?;
        println!("(Remote) Listening on {}:{}", remote_host, remote_port);

        let mut channel = ssh
            .channel_open_session()
            .await
            .with_context(|| "channel_open_session error.")?;

        while let Some(msg) = channel.wait().await {
            println!("{:?}", msg);
        }
    }

    Ok(())
}
