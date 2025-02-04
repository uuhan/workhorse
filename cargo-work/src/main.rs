use cargo_work::{
    options::*,
    ssh::{cargo, cmd, get, just, ping, pull, push, scp},
};
use clap::Parser;
use color_eyre::Result;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
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

    match cli.sub_commands {
        // 作为 cargo 子命令运行
        SubCommands::Work(w_opt) => {
            let horse = w_opt.horse;
            let scripts = w_opt.scripts;

            // cargo work -- <SCRIPTS>
            // e.g. cargo work -- ls -al
            if !scripts.is_empty() {
                if let Err(err) = cmd::run(&key, horse, scripts).await {
                    eprintln!("执行失败: {}", err);
                }
            } else if let Some(commands) = w_opt.commands {
                match commands {
                    Commands::Build(options) => {
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Zigbuild(options) => {
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Check(options) => {
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Clean(options) => {
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Clippy(options) => {
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Doc(options) => {
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Install(options) => {
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Metadata(options) => {
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Run(options) => {
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Rustc(options) => {
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Test(options) => {
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Just(options) => {
                        if let Err(err) = just::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Get(options) => {
                        if let Err(err) = get::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Scp(options) => {
                        if let Err(err) = scp::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Push(options) => {
                        if let Err(err) = push::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Pull(options) => {
                        if let Err(err) = pull::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }

                    Commands::Ping(options) => {
                        if let Err(err) = ping::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                }
            }
        }

        // 直接调用 cargo 命令
        SubCommands::Cargo(opt) => match opt {
            Commands::Build(build) => println!("{:?}", build.command()),
            Commands::Just(just) => println!("{:?}", just),
            Commands::Push(options) => {
                println!("{:?}", options);
            }
            Commands::Pull(_) => {
                let _ = cargo_work::ui::init();
            }
            opt => println!("{:?}", opt),
        },
    }

    Ok(())
}
