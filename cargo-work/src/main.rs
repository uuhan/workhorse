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
                    Commands::Build(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Zigbuild(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Check(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Clean(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Clippy(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Doc(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Install(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Metadata(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Run(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Rustc(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Test(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = cargo::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Just(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = just::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Get(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = get::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Scp(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = scp::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Push(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = push::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }
                    Commands::Pull(mut options) => {
                        merge_options(&mut options.horse, &horse);
                        if let Err(err) = pull::run(&key, options).await {
                            eprintln!("执行失败: {}", err);
                        }
                    }

                    Commands::Ping(mut options) => {
                        merge_options(&mut options.horse, &horse);
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

fn merge_options(options: &mut HorseOptions, horse: &HorseOptions) {
    if options.key.is_none() {
        options.key = horse.key.clone();
    }

    if options.repo.is_none() {
        options.repo = horse.repo.clone();
    }

    if options.remote.is_none() {
        options.remote = horse.remote.clone();
    }

    if options.repo_name.is_none() {
        options.repo_name = horse.repo_name.clone();
    }
}
