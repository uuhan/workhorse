#![allow(unused_imports)]
use clap::Parser;
use clap::{arg, command, value_parser, ArgAction, Command};
use horsed::options::*;
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
use tracing_subscriber::EnvFilter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    colored::control::set_override(true);
    let cli = Cli::parse();

    let mut work_dir = &std::env::current_dir().unwrap();

    if cli.daemon {
        let _cmd = std::process::Command::new(std::env::current_exe()?)
            .arg("-f")
            .arg("--dir")
            .arg(work_dir)
            .spawn()
            .unwrap();

        return Ok(());
    }

    if cli.foreground {
        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let file_appender = tracing_appender::rolling::never(".", "horsed.log");
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

        let ts = tracing_subscriber::fmt().with_env_filter(env_filter);

        if cli.show_log {
            ts.init();
        } else {
            ts.with_writer(non_blocking).init();
        }

        let mut tm = TaskManager::default();
        let handler = tm.spawn_essential_handle();
        let h = tm.spawn_handle();

        handler.spawn(async move {
            let db = horsed::db::db();
            if let Err(err) = Migrator::up(&db, None).await {
                tracing::error!("数据库初始化失败: {err}");
            }

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
        // 调用子命令
        match commands {
            Commands::User(sub) => {
                match sub.commands {
                    UserCommand::Add(user) => {
                        stable::prelude::handle().block_on(async move {
                            // TODO: 添加用户
                            println!("添加用户: {user:?}");
                            let db = horsed::db::db();
                        });
                    }
                    UserCommand::Del(user) => {}
                    UserCommand::Mod(user) => {}
                    UserCommand::List(user) => {}
                }
            }
        }

    } else {
        // 启动服务
        let cmd = std::process::Command::new(std::env::current_exe().unwrap())
            .current_dir(work_dir)
            .arg("--daemon")
            .spawn()?
            .wait();
    }

    Ok(())
}
