///
/// Run this example with:
/// cargo run --example client_exec_simple -- -k <private key path> <host> <command>
///
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use key::PrivateKeyWithHashAlg;
use russh::keys::*;
use russh::*;
use tokio::io::AsyncWriteExt;
use tokio::net::ToSocketAddrs;

pub async fn run() -> Result<()> {
    // Session is a wrapper around a russh client, defined down below
    let mut ssh = Session::connect(
        PathBuf::from(std::env::var("HOME")?).join(".ssh").join("id_rsa"),
        "cmd",
        "127.0.0.1:2222",
    )
    .await?;

    let code = ssh.call("ls --color=always").await?;

    println!("Exitcode: {:?}", code);
    ssh.close().await?;
    Ok(())
}

struct Client {}

// More SSH event handlers
// can be defined in this trait
// In this example, we're only using Channel, so these aren't needed.
#[async_trait]
impl client::Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(&mut self, spk: &ssh_key::PublicKey) -> Result<bool, Self::Error> {
        println!("{:?}", spk);
        Ok(true)
    }
}

/// This struct is a convenience wrapper
/// around a russh client
pub struct Session {
    session: client::Handle<Client>,
}

impl Session {
    async fn connect<P: AsRef<Path>, A: ToSocketAddrs>(
        key_path: P,
        user: impl Into<String>,
        addrs: A,
    ) -> Result<Self> {
        let key_pair = load_secret_key(key_path, None)?;
        let config = client::Config {
            inactivity_timeout: Some(Duration::from_secs(5)),
            ..<_>::default()
        };

        let config = Arc::new(config);
        let sh = Client {};

        let mut session = client::connect(config, addrs, sh).await?;
        let auth_res = session
            .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key_pair), None)?)
            .await?;

        if !auth_res {
            anyhow::bail!("Authentication failed");
        }

        Ok(Self { session })
    }

    async fn call(&mut self, command: &str) -> Result<u32> {
        let mut channel = self.session.channel_open_session().await?;
        channel.exec(true, command).await?;

        let mut code = None;
        let mut stdout = tokio::io::stdout();

        loop {
            // There's an event available on the session channel
            let Some(msg) = channel.wait().await else {
                break;
            };
            println!("{:?}", msg);
            match msg {
                // Write data to the terminal
                ChannelMsg::Data { ref data } => {
                    println!("{:?}", data);
                    stdout.write_all(data).await?;
                    stdout.flush().await?;
                }
                // The command has returned an exit code
                ChannelMsg::ExitStatus { exit_status } => {
                    println!("{:?}", exit_status);
                    code = Some(exit_status);
                    // cannot leave the loop immediately, there might still be more data to receive
                }
                _ => {}
            }
        }
        // Ok(code.expect("program did not exit cleanly"))
        Ok(0)
    }

    async fn close(&mut self) -> Result<()> {
        self.session
            .disconnect(Disconnect::ByApplication, "", "English")
            .await?;
        Ok(())
    }
}
