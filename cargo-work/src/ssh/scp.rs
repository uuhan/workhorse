use super::*;
use crate::options::ScpOptions;
use anyhow::Context;
use anyhow::Result;
use git2::Repository;
use std::ffi::OsString;
use std::path::Path;

pub async fn run(sk: &Path, options: ScpOptions) -> Result<()> {
    let repo = Repository::discover(".")?;
    let head = repo.head()?;

    let repo_name = if let Some(repo_name) = find_repo_name(&options.horse) {
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
    } else if let Some(host) = find_host(&options.horse) {
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

    let branch = head
        .shorthand()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "master".to_owned());

    #[cfg(feature = "use-system-ssh")]
    {
        let mut cmd = super::run_system_ssh(
            sk,
            &[("REPO", repo_name), ("BRANCH", branch)],
            "scp",
            host,
            [OsString::from(&options.source)],
        );
        cmd.stdout(std::process::Stdio::piped());
        let mut ssh = cmd.spawn()?;
        let mut stdout = ssh.stdout.take().unwrap();
        let mut file = tokio::fs::File::create_new(&options.dest).await?;

        while let Ok(len) = tokio::io::copy(&mut stdout, &mut file).await {
            if len == 0 {
                break;
            }
        }
    }

    Ok(())
}
