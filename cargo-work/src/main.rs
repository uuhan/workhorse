use cargo_work::ssh::{build, cmd};
use cargo_work::Build;
use clap::{Args, Parser, Subcommand};
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use std::{cmp::min, fmt::Write};

/// 命令行参数
#[derive(Clone, Debug, Parser)]
#[clap(
    version,
    name = "cargo-work",
    styles = cargo_options::styles(),
    disable_help_subcommand = true,
)]
pub struct Cli {
    #[clap(short, long, help = "显示详细信息")]
    verbose: bool,

    #[clap(flatten)]
    horse: HorseOptions,

    #[clap(subcommand)]
    commands: Commands,
}

#[derive(Clone, Debug, Args)]
pub struct HorseOptions {
    #[clap(short, long = "ssh-key", help = "指定私钥文件路径")]
    key: Option<PathBuf>,
}

#[derive(Clone, Debug, Subcommand)]
#[command(version, display_order = 1)]
pub enum Commands {
    #[command(name = "work", about = "cargo work")]
    Work(WorkOptions),
    #[command(flatten)]
    Cargo(Options),
}

#[derive(Clone, Debug, Parser)]
pub struct WorkOptions {
    #[clap(flatten)]
    horse: HorseOptions,

    #[clap(subcommand)]
    commands: Options,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Subcommand)]
#[command(version, display_order = 1)]
pub enum Options {
    #[command(name = "build", alias = "b", about = "编译项目")]
    Build(Build),
    #[command(name = "push", alias = "p", about = "推送代码到远程仓库")]
    Push,
    #[command(name = "pull", alias = "l", about = "拉取编译资产")]
    Pull,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let key = if let Some(key) = cli.horse.key.clone().take() {
        key
    } else {
        #[cfg(not(windows))]
        let home = std::env::var("HOME")?;
        #[cfg(windows)]
        let home = std::env::var("USERPROFILE")?;

        let home_dir = PathBuf::from(home);
        let path = home_dir.join(".ssh");

        if path.join("id_rsa").exists() {
            path.join("id_rsa")
        } else if path.join("id_ed25519").exists() {
            path.join("id_ed25519")
        } else {
            eprintln!("没有可以使用的私钥文件: {}", path.display());
            return Ok(());
        }
    };

    match cli.commands {
        // 作为 cargo 子命令运行
        Commands::Work(w_opt) => {
            let _horse = w_opt.horse;
            match w_opt.commands {
                Options::Build(options) => {
                    if let Err(err) = build::run(&key, options).await {
                        eprintln!("执行失败: {}", err);
                    }
                }
                Options::Push => {
                    if let Err(err) = cmd::run(&key).await {
                        eprintln!("执行失败: {}", err);
                    }
                }
                Options::Pull => {
                    let mut downloaded = 0;
                    let total_size = 23123123;

                    let pb = ProgressBar::new(total_size);
                    pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                    .unwrap()
                    .with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
                    .progress_chars("#>-"));

                    while downloaded < total_size {
                        let new = min(downloaded + 223211, total_size);
                        downloaded = new;
                        pb.set_position(new);
                        thread::sleep(Duration::from_millis(8));
                    }

                    pb.finish_with_message("downloaded");
                }
            }
        }

        // 直接调用 cargo 命令
        Commands::Cargo(opt) => match opt {
            Options::Build(build) => println!("{:?}", build.command()),
            Options::Push => {
                if let Err(err) = cmd::run(&key).await {
                    eprintln!("执行失败: {}", err);
                }
            }
            Options::Pull => {
                let mut downloaded = 0;
                let total_size = 23123123;

                let pb = ProgressBar::new(total_size);
                pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                    .unwrap()
                    .with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
                    .progress_chars("#>-"));

                while downloaded < total_size {
                    let new = min(downloaded + 223211, total_size);
                    downloaded = new;
                    pb.set_position(new);
                    thread::sleep(Duration::from_millis(8));
                }

                pb.finish_with_message("downloaded");
            }
        },
    }

    Ok(())
}
