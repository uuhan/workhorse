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

        sender.write_all(b"Hello from client!\n").await;

        Ok(())
    });

    futures::executor::block_on(tm.future());
}
