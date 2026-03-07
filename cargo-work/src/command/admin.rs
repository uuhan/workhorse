use super::*;
use crate::options::AdminOptions;
use color_eyre::eyre::{anyhow, bail, ContextCompat, Result, WrapErr};
use git2::Repository;
use std::io::Write;
use std::net::SocketAddr;
use std::path::Path;

struct AdminExecResult {
    stdout: String,
    stderr: String,
    exit_code: Option<u32>,
}

impl AdminExecResult {
    fn success(&self) -> bool {
        self.exit_code.unwrap_or(0) == 0
    }
}

pub async fn run(sk: &Path, options: AdminOptions) -> Result<()> {
    let action = "admin";
    let host = resolve_host(&options.horse)?;

    if options.command.is_empty() {
        run_interactive(sk, host, &options.horse).await
    } else {
        let trace_id = super::new_trace_id(action);
        super::log_stage(&trace_id, action, "single.start");
        let result = exec_admin(sk, host, &options.horse, &options.command, &trace_id).await?;
        print_exec_result(&result)?;
        if !result.success() {
            bail!(
                "{}",
                first_non_empty(&result.stderr, &result.stdout)
                    .unwrap_or_else(|| "admin 命令执行失败".to_string())
            );
        }
        super::log_stage(&trace_id, action, "single.done");
        Ok(())
    }
}

async fn run_interactive(sk: &Path, host: SocketAddr, horse: &HorseOptions) -> Result<()> {
    println!("进入 admin 交互模式，输入序号执行操作。");
    loop {
        print_menu();
        let choice = prompt("选择操作")?;
        let command = match choice.trim() {
            "0" | "q" | "quit" | "exit" => break,
            "1" => vec!["users".to_string(), "list".to_string()],
            "2" => {
                let name = prompt("用户名")?;
                let role = prompt_default("角色(admin/user)", "user")?;
                vec!["users".to_string(), "add".to_string(), name, role]
            }
            "3" => {
                let name = prompt("用户名")?;
                vec!["users".to_string(), "enable".to_string(), name]
            }
            "4" => {
                let name = prompt("用户名")?;
                if !confirm(&format!("确认禁用用户 {name} ?"))? {
                    continue;
                }
                vec!["users".to_string(), "disable".to_string(), name]
            }
            "5" => {
                let name = prompt("用户名")?;
                let role = prompt("新角色(admin/user)")?;
                vec!["users".to_string(), "role".to_string(), name, role]
            }
            "6" => {
                let name = prompt("用户名")?;
                if !confirm(&format!("确认删除用户 {name} ?"))? {
                    continue;
                }
                vec!["users".to_string(), "delete".to_string(), name]
            }
            "7" => {
                let name = prompt_default("按用户名过滤(留空=全部)", "")?;
                if name.is_empty() {
                    vec!["keys".to_string(), "list".to_string()]
                } else {
                    vec!["keys".to_string(), "list".to_string(), name]
                }
            }
            "8" => {
                let user = prompt("用户名")?;
                let input = prompt("公钥内容或 .pub 文件路径")?;
                let (alg, key, comment) = load_public_key_input(&input)?;
                let mut command = vec!["keys".to_string(), "add".to_string(), user, alg, key];
                if let Some(comment) = comment {
                    command.push(comment);
                }
                command
            }
            "9" => {
                let input = prompt("公钥(alg key ...)")?;
                let (alg, key, _) = parse_public_key_line(&input)?;
                vec!["keys".to_string(), "enable".to_string(), alg, key]
            }
            "10" => {
                let input = prompt("公钥(alg key ...)")?;
                let (alg, key, _) = parse_public_key_line(&input)?;
                if !confirm("确认禁用该公钥?")? {
                    continue;
                }
                vec!["keys".to_string(), "disable".to_string(), alg, key]
            }
            "11" => {
                let input = prompt("公钥(alg key ...)")?;
                let (alg, key, _) = parse_public_key_line(&input)?;
                if !confirm("确认删除该公钥?")? {
                    continue;
                }
                vec!["keys".to_string(), "delete".to_string(), alg, key]
            }
            _ => {
                eprintln!("无效输入: {choice}");
                continue;
            }
        };

        let trace_id = super::new_trace_id("admin");
        super::log_stage(&trace_id, "admin", "interactive.dispatch");
        match exec_admin(sk, host, horse, &command, &trace_id).await {
            Ok(result) => {
                print_exec_result(&result)?;
                if !result.success() {
                    eprintln!("admin 命令返回非零状态: {}", result.exit_code.unwrap_or(1));
                }
            }
            Err(err) => {
                eprintln!("执行失败: {err}");
            }
        }
    }

    println!("已退出 admin 交互模式。");
    Ok(())
}

fn print_menu() {
    println!();
    println!("1) users list");
    println!("2) users add");
    println!("3) users enable");
    println!("4) users disable");
    println!("5) users role");
    println!("6) users delete");
    println!("7) keys list");
    println!("8) keys add");
    println!("9) keys enable");
    println!("10) keys disable");
    println!("11) keys delete");
    println!("0) exit");
}

fn prompt(label: &str) -> Result<String> {
    let mut input = String::new();
    print!("{label}: ");
    std::io::stdout().flush()?;
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn prompt_default(label: &str, default_value: &str) -> Result<String> {
    let mut input = String::new();
    print!("{label} [{default_value}]: ");
    std::io::stdout().flush()?;
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim();
    if input.is_empty() {
        Ok(default_value.to_string())
    } else {
        Ok(input.to_string())
    }
}

fn confirm(label: &str) -> Result<bool> {
    let answer = prompt_default(&format!("{label} (y/N)"), "n")?;
    Ok(matches!(answer.as_str(), "y" | "Y" | "yes" | "YES"))
}

fn parse_public_key_line(line: &str) -> Result<(String, String, Option<String>)> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 2 {
        return Err(anyhow!("公钥格式错误，期望: <alg> <base64> [comment]"));
    }
    let alg = parts[0].to_string();
    let key = parts[1].to_string();
    let comment = if parts.len() > 2 {
        Some(parts[2..].join(" "))
    } else {
        None
    };
    Ok((alg, key, comment))
}

fn load_public_key_input(input: &str) -> Result<(String, String, Option<String>)> {
    let path = Path::new(input);
    if path.exists() {
        let content = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("读取公钥文件失败: {}", path.display()))?;
        let line = content
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#'))
            .context("公钥文件为空或不包含有效公钥")?;
        parse_public_key_line(line)
    } else {
        parse_public_key_line(input)
    }
}

fn resolve_host(options: &HorseOptions) -> Result<SocketAddr> {
    if let Ok(host) = std::env::var("HORSED") {
        return host
            .parse()
            .wrap_err_with(|| format!("解析环境变量 HORSED 失败: {host}"));
    }
    if let Some(host) = find_host(options) {
        return Ok(host);
    }

    let repo = Repository::discover(".")?;
    let Some(horsed) = find_remote(&repo, options) else {
        return Err(anyhow!("找不到 horsed 远程仓库!"));
    };
    horsed
        .url()
        .and_then(extract_host)
        .context("获取 horsed 远程仓库 HOST 失败")
}

async fn exec_admin(
    sk: &Path,
    host: SocketAddr,
    horse: &HorseOptions,
    args: &[String],
    trace_id: &str,
) -> Result<AdminExecResult> {
    let action = "admin";
    super::log_stage(trace_id, action, "connect.start");
    let mut ssh = HorseClient::connect(sk, horse.key_hash_alg, action, host, None, None).await?;
    let mut channel = ssh.channel_open_session().await?;
    if !trace_id.is_empty() {
        channel.set_env(true, super::TRACE_ID_ENV, trace_id).await?;
    }
    for kv in horse.env.iter() {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        channel.set_env(true, k, v).await?;
    }

    let command = args
        .iter()
        .map(|arg| shell_escape::escape(arg.clone().into()).to_string())
        .collect::<Vec<_>>()
        .join(" ");
    super::log_stage(trace_id, action, "dispatch.exec");
    channel.exec(true, command).await.wrap_err("exec")?;

    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut code = None;

    while let Some(msg) = channel.wait().await {
        match msg {
            ChannelMsg::Data { ref data } => out.extend_from_slice(data),
            ChannelMsg::ExtendedData { ref data, .. } => err.extend_from_slice(data),
            ChannelMsg::ExitStatus { exit_status } => {
                code = Some(exit_status);
                break;
            }
            _ => {}
        }
    }

    if !ssh.is_closed() {
        ssh.close().await?;
    }
    super::log_stage(trace_id, action, "done");

    Ok(AdminExecResult {
        stdout: String::from_utf8_lossy(&out).to_string(),
        stderr: String::from_utf8_lossy(&err).to_string(),
        exit_code: code,
    })
}

fn print_exec_result(result: &AdminExecResult) -> Result<()> {
    if !result.stdout.is_empty() {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(result.stdout.as_bytes())?;
        if !result.stdout.ends_with('\n') {
            stdout.write_all(b"\n")?;
        }
    }
    if !result.stderr.is_empty() {
        let mut stderr = std::io::stderr().lock();
        stderr.write_all(result.stderr.as_bytes())?;
        if !result.stderr.ends_with('\n') {
            stderr.write_all(b"\n")?;
        }
    }
    Ok(())
}

fn first_non_empty(primary: &str, fallback: &str) -> Option<String> {
    if !primary.trim().is_empty() {
        return Some(primary.trim().to_string());
    }
    if !fallback.trim().is_empty() {
        return Some(fallback.trim().to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_public_key_line;

    #[test]
    fn parse_pubkey_with_comment() {
        let (alg, key, comment) =
            parse_public_key_line("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAA test@example").unwrap();
        assert_eq!(alg, "ssh-ed25519");
        assert_eq!(key, "AAAAC3NzaC1lZDI1NTE5AAAA");
        assert_eq!(comment.as_deref(), Some("test@example"));
    }

    #[test]
    fn parse_pubkey_without_comment() {
        let (alg, key, comment) =
            parse_public_key_line("ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQ").unwrap();
        assert_eq!(alg, "ssh-rsa");
        assert_eq!(key, "AAAAB3NzaC1yc2EAAAADAQABAAABAQ");
        assert!(comment.is_none());
    }
}
