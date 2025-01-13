use horsed::ipc::*;
use interprocess::local_socket::tokio::prelude::*;
use stable::task::TaskManager;
use tokio::io::AsyncWriteExt;

fn main() {
    let mut tm = TaskManager::default();
    let handler = tm.spawn_essential_handle();

    handler.spawn(async move {
        let conn = connect().await.unwrap();

        let (_, mut sender) = conn.split();

        sender
            .write_all(
                serde_json::to_string(&data::Data::GitHook {
                    kind: "pre-commit".to_string(),
                    args: vec!["file1.txt".to_string(), "file2.txt".to_string()],
                })
                .unwrap()
                .as_bytes(),
            )
            .await;

        sender
            .write_all(serde_json::to_string(&data::Data::Exit).unwrap().as_bytes())
            .await;

        Ok(())
    });

    futures::executor::block_on(tm.future());
}
