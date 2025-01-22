use cargo_work::{
    options::*,
    ssh::{cargo, cmd, get, just, scp},
};
use clap::Parser;
use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use std::{cmp::min, fmt::Write};

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
                    if let Err(err) = cargo::run(&key, options).await {
                        eprintln!("执行失败: {}", err);
                    }
                }
                Options::Zigbuild(options) => {
                    if let Err(err) = cargo::run(&key, options).await {
                        eprintln!("执行失败: {}", err);
                    }
                }
                Options::Check(options) => {
                    if let Err(err) = cargo::run(&key, options).await {
                        eprintln!("执行失败: {}", err);
                    }
                }
                Options::Clippy(options) => {
                    if let Err(err) = cargo::run(&key, options).await {
                        eprintln!("执行失败: {}", err);
                    }
                }
                Options::Doc(options) => {
                    if let Err(err) = cargo::run(&key, options).await {
                        eprintln!("执行失败: {}", err);
                    }
                }
                Options::Install(options) => {
                    if let Err(err) = cargo::run(&key, options).await {
                        eprintln!("执行失败: {}", err);
                    }
                }
                Options::Metadata(options) => {
                    if let Err(err) = cargo::run(&key, options).await {
                        eprintln!("执行失败: {}", err);
                    }
                }
                Options::Run(options) => {
                    if let Err(err) = cargo::run(&key, options).await {
                        eprintln!("执行失败: {}", err);
                    }
                }
                Options::Rustc(options) => {
                    if let Err(err) = cargo::run(&key, options).await {
                        eprintln!("执行失败: {}", err);
                    }
                }
                Options::Test(options) => {
                    if let Err(err) = cargo::run(&key, options).await {
                        eprintln!("执行失败: {}", err);
                    }
                }
                Options::Just(options) => {
                    if let Err(err) = just::run(&key, options).await {
                        eprintln!("执行失败: {}", err);
                    }
                }
                Options::Get(options) => {
                    if let Err(err) = get::run(&key, options).await {
                        eprintln!("执行失败: {}", err);
                    }
                }
                Options::Scp(options) => {
                    if let Err(err) = scp::run(&key, options).await {
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
                } // opt => println!("{:?}", opt),
            }
        }

        // 直接调用 cargo 命令
        Commands::Cargo(opt) => match opt {
            Options::Build(build) => println!("{:?}", build.command()),
            Options::Just(just) => println!("{:?}", just),
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
            opt => println!("{:?}", opt),
        },
    }

    Ok(())
}
