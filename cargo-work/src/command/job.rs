use super::*;
use crate::options::JobOptions;
use color_eyre::eyre::{anyhow, ContextCompat, Result, WrapErr};
use git2::Repository;
use serde::Deserialize;
use std::io::{IsTerminal, Write};
use std::net::SocketAddr;
use std::path::Path;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Deserialize)]
struct JobListRow {
    id: String,
    #[serde(default)]
    running: bool,
    #[serde(default)]
    action: String,
    #[serde(default)]
    command: String,
}

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

    let mut command = if options.command.is_empty() {
        vec!["list".to_string()]
    } else {
        options.command.clone()
    };

    if should_prompt_attach_selection(&command)
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
    {
        let keep_no_follow = command.iter().any(|arg| arg == "--no-follow");
        if let Some(selected_id) = select_running_job(sk, &options, host, &trace_id).await? {
            command = vec!["attach".to_string(), selected_id];
            if keep_no_follow {
                command.push("--no-follow".to_string());
            }
        }
    }

    exec_job_streaming(sk, &options, host, &trace_id, &command).await?;
    super::log_stage(&trace_id, action, "done");
    Ok(())
}

fn should_prompt_attach_selection(command: &[String]) -> bool {
    if command.first().map(String::as_str) != Some("attach") {
        return false;
    }

    command
        .iter()
        .skip(1)
        .all(|arg| arg.as_str() == "--no-follow")
}

async fn select_running_job(
    sk: &Path,
    options: &JobOptions,
    host: SocketAddr,
    trace_id: &str,
) -> Result<Option<String>> {
    let (stdout, exit_code) =
        exec_job_capture(sk, options, host, trace_id, &["list".to_string()]).await?;
    if exit_code.unwrap_or(0) != 0 {
        return Ok(None);
    }

    let running = match parse_running_jobs(&stdout) {
        Ok(running) => running,
        Err(err) => {
            eprintln!("解析运行中任务失败: {err}");
            return Ok(None);
        }
    };

    match running.len() {
        0 => Ok(None),
        1 => Ok(Some(running[0].id.clone())),
        _ => {
            eprintln!("检测到多个运行中的任务，请选择要附着的任务:");
            for (index, job) in running.iter().enumerate() {
                let action = if job.action.is_empty() {
                    "-"
                } else {
                    job.action.as_str()
                };
                let command = if job.command.is_empty() {
                    "-"
                } else {
                    job.command.as_str()
                };
                eprintln!("  {}) {} [{}] {}", index + 1, job.id, action, command);
            }
            let selected = prompt_choice(running.len())?;
            Ok(Some(running[selected].id.clone()))
        }
    }
}

fn parse_running_jobs(stdout: &[u8]) -> Result<Vec<JobListRow>> {
    let rows =
        serde_json::from_slice::<Vec<JobListRow>>(stdout).context("解析服务端任务列表失败")?;
    Ok(rows.into_iter().filter(|job| job.running).collect())
}

fn prompt_choice(total: usize) -> Result<usize> {
    loop {
        print!("输入序号 [1-{total}]: ");
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if let Ok(n) = input.parse::<usize>() {
            if (1..=total).contains(&n) {
                return Ok(n - 1);
            }
        }

        eprintln!("无效输入: {input}");
    }
}

async fn exec_job_streaming(
    sk: &Path,
    options: &JobOptions,
    host: SocketAddr,
    trace_id: &str,
    command: &[String],
) -> Result<()> {
    super::log_stage(trace_id, "job", "connect.start");
    let mut ssh =
        HorseClient::connect(sk, options.horse.key_hash_alg, "job", host, None, None).await?;
    let mut channel = ssh.channel_open_session().await?;
    if !trace_id.is_empty() {
        channel.set_env(true, super::TRACE_ID_ENV, trace_id).await?;
    }
    for kv in options.horse.env.iter() {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        channel.set_env(true, k, v).await?;
    }

    let command_line = command.join(" ");
    super::log_stage(trace_id, "job", "dispatch.exec");
    channel
        .exec(true, command_line.as_bytes())
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

    Ok(())
}

async fn exec_job_capture(
    sk: &Path,
    options: &JobOptions,
    host: SocketAddr,
    trace_id: &str,
    command: &[String],
) -> Result<(Vec<u8>, Option<u32>)> {
    let mut ssh =
        HorseClient::connect(sk, options.horse.key_hash_alg, "job", host, None, None).await?;
    let mut channel = ssh.channel_open_session().await?;
    if !trace_id.is_empty() {
        channel.set_env(true, super::TRACE_ID_ENV, trace_id).await?;
    }
    for kv in options.horse.env.iter() {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        channel.set_env(true, k, v).await?;
    }

    let command_line = command.join(" ");
    channel
        .exec(true, command_line.as_bytes())
        .await
        .wrap_err("exec")?;

    let mut stdout = Vec::new();
    let mut exit_code = None;
    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { ref data } => {
                stdout.extend_from_slice(data);
            }
            ChannelMsg::ExitStatus { exit_status } => {
                exit_code = Some(exit_status);
            }
            ChannelMsg::Close | ChannelMsg::Eof => {}
            _ => {}
        }
    }

    if !ssh.is_closed() {
        ssh.close().await?;
    }

    Ok((stdout, exit_code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_prompt_attach_for_attach_without_id() {
        let command = vec!["attach".to_string()];
        assert!(should_prompt_attach_selection(&command));
    }

    #[test]
    fn should_prompt_attach_for_attach_with_no_follow_only() {
        let command = vec!["attach".to_string(), "--no-follow".to_string()];
        assert!(should_prompt_attach_selection(&command));
    }

    #[test]
    fn should_not_prompt_attach_for_explicit_id() {
        let command = vec!["attach".to_string(), "job-1".to_string()];
        assert!(!should_prompt_attach_selection(&command));
    }

    #[test]
    fn should_not_prompt_attach_for_other_subcommands() {
        let command = vec!["list".to_string()];
        assert!(!should_prompt_attach_selection(&command));
    }

    #[test]
    fn parse_running_jobs_filters_non_running_rows() {
        let raw = br#"[
            {"id":"job-1","running":true,"action":"cargo","command":"build"},
            {"id":"job-2","running":false,"action":"cmd","command":"ls"}
        ]"#;

        let running = parse_running_jobs(raw).unwrap();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id, "job-1");
    }
}
