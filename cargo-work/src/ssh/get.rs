use super::*;
use crate::options::GetOptions;
use anyhow::Context;
use anyhow::Result;
use git2::Repository;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use stable::data::{v1::*, *};
use std::ffi::OsString;
use std::fmt::Write;
use std::path::Path;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn run(sk: &Path, options: GetOptions) -> Result<()> {
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
        use clean_path::Clean;
        let current_dir = std::env::current_dir().unwrap();
        let mut file_path = current_dir.join(PathBuf::from(&options.file)).clean();

        if let Some(dir) = file_path.parent() {
            if !dir.exists() {
                std::fs::create_dir_all(dir).context(format!("创建目录失败: {}", dir.display()))?;
            }
        }

        let mut cmd = super::run_system_ssh(
            sk,
            &[("REPO", repo_name), ("BRANCH", branch)],
            "get",
            host,
            [OsString::from(&options.file)],
        );
        cmd.stdout(std::process::Stdio::piped());
        let mut ssh = cmd.spawn()?;
        let mut stdout = ssh.stdout.take().unwrap();

        let mut header = [0u8; HEADER_SIZE];
        stdout.read_exact(&mut header).await?;
        let header = Header::ref_from_bytes(&header).unwrap();

        let mut get_file_info = vec![0u8; header.size as usize];
        stdout.read_exact(&mut get_file_info).await?;

        let get_file = bincode::deserialize::<GetFile>(&get_file_info)?;

        // println!("文件信息: {}", get_file.path.display());
        // println!("文件大小: {}", get_file.size);

        if get_file.kind.is_file() && file_path.exists() && !options.force {
            return Err(anyhow::anyhow!("文件已存在: {}", file_path.display()));
        }

        if get_file.kind.is_dir() {
            file_path.set_extension("tar.zip");
        }

        let pb = if let Some(size) = get_file.size {
            ProgressBar::new(size)
        } else {
            ProgressBar::no_length()
        };

        let mut file = tokio::fs::File::create(&file_path).await?;
        let mut downloaded: u64 = 0;

        pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
            .progress_chars("#>-"));

        const BUF_SIZE: usize = 1024 * 32;
        let mut buf = [0u8; BUF_SIZE];

        while let Ok(len) = stdout.read(&mut buf).await {
            pb.set_position(downloaded);

            if len == 0 {
                break;
            }

            downloaded += len as u64;
            file.write_all(&buf[..len]).await?;
        }

        pb.finish();
    }

    Ok(())
}
