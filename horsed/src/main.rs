#![allow(unused_imports)]
use anyhow::Context;
use clap::Parser;
use clap::{arg, command, value_parser, ArgAction, Command};
use horsed::{options::*, prelude::*};
use interprocess::local_socket::{
    tokio::{prelude::*, Stream},
    GenericNamespaced, ListenerOptions,
};
use migration::{Migrator, MigratorTrait};
use stable::prelude::*;
use std::io;
use std::path::PathBuf;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    try_join,
};
use tracing_appender::non_blocking::WorkerGuard;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: 从配置文件中读取并设置一些环境变量
    // std::env::set_var("CARGO_TERM_COLOR", "always");

    colored::control::set_override(true);
    let cli = Cli::parse();

    let work_dir = &std::env::current_dir().unwrap();

    if cli.daemon {
        let mut cmd = std::process::Command::new(std::env::current_exe()?);

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            const DETACHED_PROCESS: u32 = 0x00000008;

            cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
        }

        cmd.arg("-f")
            .arg("--dir")
            .arg(work_dir)
            .spawn()
            .context("启动服务失败")?;

        return Ok(());
    }

    if cli.foreground {
        let mut tm = TaskManager::default();
        let mut tm1 = TaskManager::default();

        let handler = tm.spawn_essential_handle();
        let task = tm.spawn_handle();
        let h = tm.spawn_handle();

        task.spawn(async move {
            horsed::logger::init(cli.show_log);

            tracing::info!("数据库初始化...");
            let db = horsed::db::db();
            if let Err(err) = Migrator::up(&db, None).await {
                tracing::error!("数据库初始化失败: {err}");
            }
            Ok(())
        });

        let in_danger = std::env::var("DANGEROUS")
            .map(|v| v == "1")
            .unwrap_or(false)
            || cli.dangerous;

        if !key_exists() || in_danger {
            tracing::warn!("临时服务启动...");
            let handler = tm1.spawn_essential_handle();
            let h = handler.clone();

            handler.spawn(async move {
                horsed::ssh::setup::run(h, in_danger).await?;
                Ok(())
            });

            if !in_danger {
                stable::prelude::handle().block_on(tm1.future())?;
                tm1.terminate();
                tracing::info!("临时服务退出...");
            } else {
                tracing::warn!("临时服务常驻...");
            }
        }

        tracing::info!("正式服务启动中...");

        handler.spawn(async move {
            horsed::ssh::run().await?;
            Ok(())
        });

        handler.spawn(async move {
            use horsed::ipc::*;
            tracing::info!("IPC Server Running...");

            let listener = listen().await?;
            loop {
                let conn = match listener.accept().await {
                    Ok(c) => c,
                    Err(err) => {
                        tracing::info!("Error while accepting connection: {err}");
                        continue;
                    }
                };

                tracing::info!("IPC 新连接");

                h.spawn(async move {
                    let mut recver = BufReader::new(&conn);

                    // Allocate a sizeable buffer for receiving. This size should be big enough and easy to
                    // find for the allocator.
                    let mut buffer = String::with_capacity(128);

                    // Describe the receive operation as receiving a line into our big buffer.
                    let recv = recver.read_line(&mut buffer).await?;

                    tracing::info!("接收 IPC 消息: [{recv}] {}", buffer.trim());
                    Ok(())
                });
            }
        });

        stable::prelude::handle().block_on(tm.future())?;

        return Ok(());
    }

    if let Some(commands) = cli.commands {
        use horsed::db::entity::{self, prelude::*, user};
        use sea_orm::entity::prelude::*;
        use sea_orm::ActiveModelTrait;
        use sea_orm::ActiveValue::{NotSet, Set, Unchanged};
        use sea_orm::{EntityTrait, ModelTrait};
        // 调用子命令
        match commands {
            Commands::User(sub) => {
                match sub.commands {
                    UserCommand::Add(user) => {
                        stable::prelude::handle().block_on(async move {
                            // TODO: 添加用户
                            let db = horsed::db::db();
                            let user = entity::user::ActiveModel {
                                name: Set(user.name),
                                ..Default::default()
                            };
                            match user.insert(&db).await {
                                Ok(user) => {
                                    println!("用户添加成功: name: {}, id: {}", user.name, user.id);
                                }
                                Err(err) => {
                                    eprintln!("添加用户失败: {err}");
                                }
                            }
                        });
                    }
                    UserCommand::Del(user) => {
                        stable::prelude::handle().block_on(async move {
                            let db = horsed::db::db();
                            match User::find()
                                .filter(user::Column::Name.eq(&user.name))
                                .one(&db)
                                .await
                            {
                                Ok(Some(user)) => {
                                    let id = user.id;
                                    match user.delete(&db).await {
                                        Ok(_) => {
                                            println!("用户删除成功: {}", id);
                                        }
                                        Err(err) => {
                                            eprintln!("删除用户失败: {err}");
                                        }
                                    }
                                }
                                Ok(None) => {
                                    eprintln!("用户不存在: {}", user.name);
                                }
                                Err(err) => {
                                    eprintln!("查找用户失败: {err}");
                                }
                            }
                        });
                    }
                    UserCommand::Mod(_user) => {}
                    UserCommand::List(_user) => {}
                }
            }
        }
    } else {
        // 启动服务
        let mut cmd = std::process::Command::new(std::env::current_exe()?);

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            const DETACHED_PROCESS: u32 = 0x00000008;

            cmd.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
        }

        cmd.current_dir(work_dir).arg("--daemon").spawn()?.wait()?;
    }

    Ok(())
}
