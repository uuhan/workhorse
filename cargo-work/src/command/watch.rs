use super::*;
use crate::options::WatchOptions;
use color_eyre::eyre::{anyhow, ContextCompat, Result};
use futures::{
    channel::mpsc::{channel, Receiver},
    SinkExt, StreamExt,
};
use git2::Repository;
use notify::{
    event::{CreateKind, DataChange, ModifyKind, RemoveKind, RenameMode},
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use std::path::Path;

pub async fn run(sk: &Path, options: WatchOptions) -> Result<()> {
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
        host.parse()?
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

    let (mut watcher, mut rx) = async_watcher()?;

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher.watch(
        options
            .path
            .unwrap_or(repo.path().parent().unwrap().to_path_buf())
            .as_ref(),
        RecursiveMode::Recursive,
    )?;

    while let Some(res) = rx.next().await {
        match res {
            Ok(event) => {
                // {file} {dir}
                let command = options.commands.join(" ");
                let paths = event.paths;

                tracing::debug!(kind=?event.kind, paths = ?paths, "{command}");
                match event.kind {
                    EventKind::Create(CreateKind::File) => {}
                    EventKind::Remove(RemoveKind::File) => {}
                    EventKind::Modify(ModifyKind::Data(DataChange::Content)) => {}
                    EventKind::Modify(ModifyKind::Data(_)) => {}

                    EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {}
                    EventKind::Modify(ModifyKind::Name(_)) => {}

                    EventKind::Remove(RemoveKind::Folder) => {}
                    EventKind::Create(CreateKind::Folder) => {}

                    EventKind::Remove(RemoveKind::Any) => {}
                    EventKind::Remove(RemoveKind::Other) => {}
                    EventKind::Create(CreateKind::Any) => {}
                    EventKind::Create(CreateKind::Other) => {}
                    EventKind::Modify(ModifyKind::Any) => {}
                    EventKind::Modify(ModifyKind::Other) => {}

                    EventKind::Modify(ModifyKind::Metadata(_)) => {}
                    EventKind::Access(_) => {}

                    EventKind::Any => {}
                    EventKind::Other => {}
                }

                drop(command);
            }
            Err(e) => tracing::info!("watch error: {:?}", e),
        }
    }

    Ok(())
}

fn async_watcher() -> notify::Result<(RecommendedWatcher, Receiver<notify::Result<Event>>)> {
    let (mut tx, rx) = channel(1);

    // Automatically select the best implementation for your platform.
    // You can also access each implementation directly e.g. INotifyWatcher.
    let watcher = RecommendedWatcher::new(
        move |res| {
            futures::executor::block_on(async {
                tx.send(res).await.unwrap();
            })
        },
        Config::default(),
    )?;

    Ok((watcher, rx))
}
