use super::*;
use crate::options::HealthOptions;
use color_eyre::eyre::WrapErr;
use color_eyre::eyre::{anyhow, ContextCompat, Result};
use git2::Repository;
use stable::data::v2::{self, Body};
use std::path::Path;
use tokio::io::AsyncWriteExt;
use zerocopy::IntoBytes;

pub async fn run(sk: &Path, mut options: HealthOptions) -> Result<()> {
    let repo = Repository::discover(".")?;

    if let Some(remote) = options.remote {
        options.horse.remote.replace(remote);
    }

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

    let mut ssh = HorseClient::connect(sk, options.horse.key_hash_alg, "health", host, None, None).await?;
    let mut channel = ssh.channel_open_session().await?;

    channel.exec(true, &[]).await.wrap_err("ssh exec")?;

    let mut sshin = channel.make_writer();
    let mut sshout = channel.make_reader();

    let req = bincode::serialize(&Body::HealthCheck)?;
    let head = v2::head(req.len() as _);

    sshin.write_all(head.as_bytes()).await?;
    sshin.write_all(&req).await?;

    let body = Body::read(&mut sshout).await?;
    match body {
        Body::HealthStatus { ulimit } => {
            tracing::info!("Health OK.");
            if let Some(lim) = ulimit {
                tracing::info!("Server ulimit -n: {}", lim);
            } else {
                tracing::info!("Server ulimit -n: unknown");
            }
        }
        _ => {
            return Err(anyhow!("health 失败, 收到非预期的响应"));
        }
    }

    ssh.close().await?;

    Ok(())
}
