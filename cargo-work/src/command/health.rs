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
    let action = "health";
    let trace_id = super::new_trace_id(action);
    super::log_stage(&trace_id, action, "resolve.start");
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
    super::log_stage(&trace_id, action, "resolve.done");

    let body =
        match call_health_once(sk, &options.horse, host, &trace_id, Body::HealthCheckV2).await {
            Ok(body) => body,
            Err(err) => {
                tracing::warn!("health v2 失败, 回退到 v1: {}", err);
                call_health_once(sk, &options.horse, host, &trace_id, Body::HealthCheck).await?
            }
        };
    match body {
        Body::HealthStatusV2 {
            ulimit,
            version,
            commit,
            os,
            arch,
            family,
            default_shell,
        } => {
            tracing::info!("Health OK.");
            tracing::info!("Server version: {} ({})", version, commit);
            tracing::info!("Server OS: {} / {} ({})", os, arch, family);
            tracing::info!(
                "Server default shell: {}",
                default_shell.unwrap_or_else(|| "unknown".to_string())
            );
            if let Some(lim) = ulimit {
                tracing::info!("Server ulimit -n: {}", lim);
            } else {
                tracing::info!("Server ulimit -n: unknown");
            }
        }
        Body::HealthStatus { ulimit } => {
            tracing::info!("Health OK (legacy).");
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
    super::log_stage(&trace_id, action, "done");

    Ok(())
}

async fn call_health_once(
    sk: &Path,
    horse: &crate::options::HorseOptions,
    host: std::net::SocketAddr,
    trace_id: &str,
    req_body: Body,
) -> Result<Body> {
    super::log_stage(trace_id, "health", "connect.start");
    let mut ssh = HorseClient::connect(sk, horse.key_hash_alg, "health", host, None, None).await?;
    let mut channel = ssh.channel_open_session().await?;
    if !trace_id.is_empty() {
        channel.set_env(true, super::TRACE_ID_ENV, trace_id).await?;
    }

    channel.exec(true, &[]).await.wrap_err("ssh exec")?;

    let mut sshin = channel.make_writer();
    let mut sshout = channel.make_reader();

    let req = bincode::serialize(&req_body)?;
    let head = v2::head(req.len() as _);
    sshin.write_all(head.as_bytes()).await?;
    sshin.write_all(&req).await?;

    let body = Body::read(&mut sshout).await?;
    ssh.close().await?;
    Ok(body)
}
