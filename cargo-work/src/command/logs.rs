use super::*;
use crate::options::LogsOptions;
use color_eyre::eyre::{anyhow, ContextCompat, Result, WrapErr};
use git2::Repository;
use std::path::Path;

pub async fn run(sk: &Path, options: LogsOptions) -> Result<()> {
    let action = "logs";
    let trace_id = super::new_trace_id(action);
    super::log_stage(&trace_id, action, "resolve.start");
    let repo = Repository::discover(".")?;
    let head = repo.head()?;

    let host = if let Ok(host) = std::env::var("HORSED") {
        host.parse()
            .context(format!("解析环境变量 HORSED 失败: {host}"))?
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

    super::log_stage(&trace_id, action, "connect.start");
    let mut ssh =
        HorseClient::connect(sk, options.horse.key_hash_alg, "logs", host, None, None).await?;
    let mut channel = ssh.channel_open_session().await?;
    if !trace_id.is_empty() {
        channel
            .set_env(true, super::TRACE_ID_ENV, &trace_id)
            .await?;
    }
    for kv in options.horse.env.iter() {
        let (k, v) = kv.split_once('=').unwrap_or_else(|| (kv, ""));
        channel.set_env(true, k, v).await?;
    }

    let commands = if options.forward {
        vec!["logs", "-f"]
    } else {
        vec!["logs"]
    };

    channel
        .exec(true, commands.join(" "))
        .await
        .wrap_err("exec")?;
    super::log_stage(&trace_id, action, "dispatch.exec");

    tokio::io::copy(&mut channel.make_reader(), &mut tokio::io::stdout()).await?;
    ssh.close().await?;
    super::log_stage(&trace_id, action, "done");

    Ok(())
}
