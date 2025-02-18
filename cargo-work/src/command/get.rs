use super::*;
use crate::options::GetOptions;
use clean_path::Clean;
use color_eyre::eyre::{anyhow, ContextCompat, Result, WrapErr};
use flate2::write::ZlibDecoder;
use fs4::fs_std::FileExt;
use git2::Repository;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use stable::data::{v2::*, *};
use std::io::IsTerminal;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use tokio::io::AsyncReadExt;

pub async fn run(sk: &Path, options: GetOptions) -> Result<()> {
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

    let branch = head
        .shorthand()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "master".to_owned());

    let current_dir = std::env::current_dir().unwrap();
    let mut file_path = current_dir.join(PathBuf::from(&options.file)).clean();

    #[cfg(not(feature = "use-system-ssh"))]
    let mut channel = {
        let ssh =
            HorseClient::connect(sk, options.horse.key_hash_alg, "get", host, None, None).await?;
        let channel = ssh.channel_open_session().await?;
        channel.set_env(true, "REPO", repo_name).await?;
        channel.set_env(true, "BRANCH", branch).await?;

        channel
            .exec(true, options.file.as_bytes())
            .await
            .wrap_err("exec")?;

        channel
    };
    #[cfg(not(feature = "use-system-ssh"))]
    let mut stdout = channel.make_reader();

    #[cfg(feature = "use-system-ssh")]
    let mut ssh = {
        let mut cmd = super::run_system_ssh(
            sk,
            &[("REPO", repo_name), ("BRANCH", branch)],
            "get",
            host,
            [std::ffi::OsString::from(&options.file)],
        );

        cmd.kill_on_drop(true);
        cmd.stdout(std::process::Stdio::piped());
        cmd.spawn()?
    };
    #[cfg(feature = "use-system-ssh")]
    let mut stdout = ssh.stdout.take().unwrap();

    let head = Head::read(&mut stdout).await?;
    let mut body = vec![0u8; head.size as usize];
    stdout.read_exact(&mut body).await?;

    let get_file = if let Ok(get_file) = bincode::deserialize::<GetFile>(&body) {
        get_file
    } else if let Ok(body) = bincode::deserialize::<Body>(&body) {
        match body {
            Body::GetFile(get_file) => get_file,
            body => {
                return Err(anyhow!("获取文件错误: {}", serde_json::to_string(&body)?));
            }
        }
    } else {
        return Err(anyhow!("协议错误: {:?} {:?}", head, body));
    };

    let is_piped = !std::io::stdout().is_terminal();

    if get_file.kind.is_file()
        && file_path.exists()
        && !options.force
        && !options.stdout
        && !is_piped
        && options.outfile.is_none()
    // Do Not Overwrite File
    {
        return Err(anyhow!("文件已存在: {}", file_path.display()));
    }

    if get_file.kind.is_dir() {
        file_path.set_extension("tar");
    }

    let mut downloaded: u64 = 0;

    let pb = if let Some(size) = get_file.size {
        ProgressBar::new(size)
    } else {
        ProgressBar::no_length()
    };

    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .with_key("eta", |state: &ProgressState, w: &mut dyn std::fmt::Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
            .progress_chars("#>-"));

    const BUF_SIZE: usize = 1024 * 32;
    let mut buf = [0u8; BUF_SIZE];

    if options.stdout || is_piped {
        let mut decoder = ZlibDecoder::new(std::io::stdout());

        while let Ok(len) = stdout.read(&mut buf).await {
            pb.set_position(downloaded);

            if len == 0 {
                break;
            }

            downloaded += len as u64;
            decoder.write_all(&buf[..len])?;
        }

        decoder.finish()?;
    } else {
        // use user specified output file
        let file_path = options.outfile.unwrap_or(file_path);

        if let Some(dir) = file_path.parent() {
            if !dir.exists() {
                std::fs::create_dir_all(dir).context(format!("创建目录失败: {}", dir.display()))?;
            }
        }

        let file = std::fs::File::create(&file_path)?;
        file.try_lock_exclusive().wrap_err("文件锁定失败!")?;
        let mut decoder = ZlibDecoder::new(file);

        while let Ok(len) = stdout.read(&mut buf).await {
            pb.set_position(decoder.total_out());

            if len == 0 {
                break;
            }

            decoder.write_all(&buf[..len])?;
        }

        decoder.finish()?;
    };

    pb.finish();

    Ok(())
}
