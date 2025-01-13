use crate::prelude::*;
use interprocess::local_socket::{
    tokio::{prelude::*, Listener, Stream},
    GenericNamespaced, ListenerOptions,
};
use std::io::ErrorKind::AddrInUse;

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    try_join,
};
pub mod data;

static IPC: &str = "horsed.sock";

/// 创建一个 ipc 监听
pub async fn listen() -> HorseResult<Listener> {
    let ipc_name = IPC.to_ns_name::<GenericNamespaced>()?;
    let opts = ListenerOptions::new().name(ipc_name);

    let listener = match opts.create_tokio() {
        Err(err) if err.kind() == AddrInUse => {
            // TODO: 程序没有正确退出, 导致 socket 文件残留
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
    use super::{connect, listen, run, IPC};
    use interprocess::local_socket::{
        tokio::{prelude::*, Stream},
        GenericFilePath, GenericNamespaced,
    };
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

        futures::executor::block_on(tm.future());
    }
}
