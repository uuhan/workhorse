use super::*;
use crate::options::JobOptions;
use color_eyre::eyre::{anyhow, ContextCompat, Result, WrapErr};
use git2::Repository;
use std::path::Path;
use tokio::io::AsyncWriteExt;

pub async fn run(sk: &Path, options: JobOptions) -> Result<()> {
    let action = "job";
    let trace_id = super::new_trace_id(action);
    super::log_stage(&trace_id, action, "resolve.start");
    let repo = Repository::discover(".")?;

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

    super::log_stage(&trace_id, action, "connect.start");
    let mut ssh =
        HorseClient::connect(sk, options.horse.key_hash_alg, "job", host, None, None).await?;
    let mut channel = ssh.channel_open_session().await?;
    if !trace_id.is_empty() {
        channel
            .set_env(true, super::TRACE_ID_ENV, &trace_id)
            .await?;
    }
    for kv in options.horse.env.iter() {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        channel.set_env(true, k, v).await?;
    }

    let command = if options.command.is_empty() {
        "list".to_string()
    } else {
        options.command.join(" ")
    };
    super::log_stage(&trace_id, action, "dispatch.exec");
    channel
        .exec(true, command.as_bytes())
        .await
        .wrap_err("exec")?;

    let mut stdout = tokio::io::stdout();
    let mut stderr = tokio::io::stderr();
    let mut code = 0_u32;
    let mut got_exit_status = false;

    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { ref data } => {
                stdout.write_all(data).await?;
                stdout.flush().await?;
            }
            ChannelMsg::ExtendedData { ref data, .. } => {
                stderr.write_all(data).await?;
                stderr.flush().await?;
            }
            ChannelMsg::ExitStatus { exit_status } => {
                got_exit_status = true;
                code = exit_status;
            }
            ChannelMsg::Close | ChannelMsg::Eof => {}
            _ => {}
        }
    }

    if !ssh.is_closed() {
        ssh.close().await?;
    }
    if got_exit_status && code != 0 {
        return Err(anyhow!("remote job command failed with exit status {code}"));
    }
    super::log_stage(&trace_id, action, "done");
    Ok(())
}
