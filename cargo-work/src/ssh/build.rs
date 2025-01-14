use super::HorseClient;
use crate::Build;
use anyhow::Context;
use anyhow::Result;
use git2::Remote;
use git2::Repository;
use russh::ChannelMsg;
use std::net::SocketAddr;
use std::path::Path;
use tokio::io::AsyncWriteExt;
use url::{Host, Position, Url};

/// 获取默认的配置, 目前 `牛马` 设置远程仓库名为 `horsed` 或 `just-horsed`
fn find_remote(repo: &Repository) -> Option<Remote<'_>> {
    if let Ok(remote) = repo.find_remote("horsed") {
        return Some(remote);
    }

    if let Ok(remote) = repo.find_remote("just-horsed") {
        return Some(remote);
    }

    None
}

fn find_repo_name(options: &Build) -> Option<String> {
    // 如果参数中指定了远程仓库, 则使用参数指定的仓库
    if let Some(ref repo_name) = options.horse.repo_name {
        return Some(repo_name.to_string());
    }

    // 如果参数中指定了远程仓库的 URL, 则使用参数指定的 URL
    if let Some(ref url) = options.horse.repo {
        return extract_repo_name(url);
    }

    None
}

fn find_host(options: &Build) -> Option<SocketAddr> {
    options.horse.repo.as_ref().and_then(|s| extract_host(s))
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

pub async fn run(sk: &Path, options: Build) -> Result<()> {
    let repo = Repository::discover(".")?;
    let head = repo.head()?;

    let repo_name = if let Some(repo_name) = find_repo_name(&options) {
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
    } else if let Some(host) = find_host(&options) {
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

    let mut ssh = HorseClient::connect(sk, "cargo", host).await?;

    let branch = head
        .shorthand()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "master".to_owned());

    let mut channel = ssh.channel_open_session().await?;
    channel.set_env(true, "REPO", repo_name).await?;
    channel.set_env(true, "BRANCH", branch).await?;
    channel
        .set_env(true, "CARGO_BUILD", serde_json::to_string(&options.cargo)?)
        .await?;
    channel.exec(true, "build").await?;

    let mut code = None;
    let mut stdout = tokio::io::stdout();

    loop {
        // There's an event available on the session channel
        let Some(msg) = channel.wait().await else {
            break;
        };
        match msg {
            // Write data to the terminal
            ChannelMsg::Data { ref data } => {
                stdout.write_all(data).await?;
                stdout.flush().await?;
            }
            // The command has returned an exit code
            ChannelMsg::ExitStatus { exit_status } => {
                code = Some(exit_status);
            }
            e => {
                eprintln!("unexpected message: {:?}", e);
            }
        }
    }

    ssh.close().await?;
    code.context("program did not exit cleanly")?;

    Ok(())
}
