#![allow(unused_imports)]
use clap::{arg, command, value_parser, ArgAction, Command};
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
    let matches = command!()
        .arg(arg!(
            -f --fg "Run in the foreground"
        ))
        .arg(
            arg!(
                -d --dir <DIR> "Set the working directory"
            )
            .value_parser(value_parser!(PathBuf)),
        )
        .arg(arg!(
            --daemon "Run in background"
        ))
        .subcommand(
            Command::new("ls")
                .about("List tasks")
                .arg(arg!(-l --list "list tasks in details").action(ArgAction::SetTrue)),
        )
        .get_matches();

    let mut work_dir = &std::env::current_dir().unwrap();

    if let Some(dir) = matches.get_one::<PathBuf>("dir") {
        work_dir = dir;
    }

    if matches.get_flag("daemon") {
        let _cmd = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("-f")
            .arg("--dir")
            .arg(work_dir)
            .spawn()
            .unwrap();

        return Ok(());
    }

    if matches.get_flag("fg") {
        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_test_writer()
            .init();
        let mut tm = TaskManager::default();
        let handler = tm.spawn_essential_handle();
        let h = tm.spawn_handle();

        handler.spawn(async move {
            let db = horsed::db::db();
            Migrator::up(&db, None).await;
            horsed::ssh::run().await;
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

        futures::executor::block_on(tm.future())?;

        return Ok(());
    }

    match matches.subcommand_matches("ls") {
        Some(matches) => {
            if matches.get_flag("list") {
                println!("Printing testing lists...");
            } else {
                println!("Not printing testing lists...");
            }

            horsed::command::task::run(matches);
        }
        None => {}
    }

    let cmd = std::process::Command::new(std::env::current_exe().unwrap())
        .current_dir(work_dir)
        .arg("--daemon")
        .spawn()?
        .wait();

    Ok(())
}
