#![allow(unused_imports)]
use clap::{arg, command, value_parser, ArgAction, Command};
use interprocess::local_socket::{
    tokio::{prelude::*, Stream},
    GenericNamespaced, ListenerOptions,
};
use std::io;
use std::path::PathBuf;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    try_join,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        horsed::ui::run().await;
        return Ok(());
    }

    match matches.subcommand_matches("ls") {
        Some(matches) => {
            if matches.get_flag("list") {
                println!("Printing testing lists...");
            } else {
                println!("Not printing testing lists...");
            }

            horsed::command::ls::run(matches);
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
