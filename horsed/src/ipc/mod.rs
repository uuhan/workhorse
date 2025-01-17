use crate::prelude::*;
use anyhow::Context;
use interprocess::local_socket::{
    tokio::{prelude::*, Listener, Stream},
    GenericNamespaced, ListenerOptions,
};
use std::io::ErrorKind::AddrInUse;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
pub mod data;

static IPC: &str = "horsed.sock";

/// 创建一个 ipc 监听
pub async fn listen() -> HorseResult<Listener> {
    let ipc_name = IPC.to_ns_name::<GenericNamespaced>()?;
    let opts = ListenerOptions::new().name(ipc_name);

    let listener = match opts.create_tokio() {
        // ipc 已被占用
        Err(err) if err.kind() == AddrInUse => {
            let stream = match connect().await {
                Err(err) => {
                    // FIXME: interprocess 库在 macOS 上退出不会自动清理 ipc 文件
                    #[cfg(target_os = "macos")]
                    {
                        // 尝试连接 ipc, 如果失败, 则删除 ipc 文件
                        let ipc_path = std::path::PathBuf::from("/tmp").join(IPC);
                        if ipc_path.exists() {
                            std::fs::remove_file(ipc_path)?;
                        }

                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        tracing::info!("尝试重新连接: {}", IPC);
                        return Box::pin(listen()).await;
                    }

                    Err(err).context("FIXME: IPC 既无法创建, 也无法连接.")?
                }
                x => x?,
            };

            // TODO: 检查连接状态
            let _ = stream;

            return Err(err.into());
        }
        x => x?,
    };

    Ok(listener)
}

/// 创建一个 ipc 连接
pub async fn connect() -> HorseResult<Stream> {
    let name = IPC.to_ns_name::<GenericNamespaced>().unwrap();
    Ok(Stream::connect(name).await?)
}

pub async fn run() -> HorseResult<()> {
    loop {
        let conn = match listen().await?.accept().await {
            Ok(c) => c,
            Err(err) => {
                tracing::info!("Error while accepting connection: {err}");
                continue;
            }
        };

        tokio::spawn(async move {
            if let Err(err) = handle_conn(conn).await {
                tracing::error!("Error while handling connection: {err}");
            }
        });
    }
}

async fn handle_conn(conn: Stream) -> HorseResult<()> {
    let mut recver = BufReader::new(&conn);
    let mut sender = &conn;

    // Allocate a sizeable buffer for receiving. This size should be big enough and easy to
    // find for the allocator.
    let mut buffer = String::with_capacity(128);

    // Describe the receive operation as receiving a line into our big buffer.
    let recv = recver.read_line(&mut buffer).await;
    sender.write_all(b"Hello from server!\n").await?;

    // Produce our output!
    println!("Client answered: {}", buffer.trim());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{connect, listen, IPC};
    use interprocess::local_socket::tokio::prelude::*;
    use stable::prelude::handle;
    use stable::task::TaskManager;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        try_join,
    };

    #[test]
    fn test_ipc_stream() {
        let mut tm = TaskManager::default();
        let handler = tm.spawn_essential_handle();
        let h = handler.clone();
        handler.spawn(async move {
            let listener = listen().await.unwrap();

            h.spawn(async move {
                println!("Connecting to {}", IPC);
                let conn = connect().await.unwrap();

                // This consumes our connection and splits it into two halves, so that we can concurrently use
                // both.
                let (recver, mut sender) = conn.split();
                let mut recver = BufReader::new(recver);

                // Allocate a sizeable buffer for receiving. This size should be enough and should be easy to
                // find for the allocator.
                let mut buffer = String::with_capacity(128);

                // Describe the send operation as writing our whole string.
                let send = sender.write_all(b"Hello from client!\n");
                // Describe the receive operation as receiving until a newline into our buffer.
                let recv = recver.read_line(&mut buffer);

                // Concurrently perform both operations.
                try_join!(send, recv).unwrap();

                // Close the connection a bit earlier than you'd think we would. Nice practice!
                drop((recver, sender));

                // Display the results when we're done!
                println!("Server answered: {}", buffer.trim());
                Ok(())
            });

            loop {
                let conn = match listener.accept().await {
                    Ok(c) => c,
                    Err(err) => {
                        eprintln!("Error while accepting connection: {err}");
                        continue;
                    }
                };

                h.spawn(async move {
                    if let Err(err) = super::handle_conn(conn).await {
                        eprintln!("Error while handling connection: {err}");
                    }
                    Ok(())
                });
            }
        });

        let _ = handle().block_on(tm.future());
    }
}
