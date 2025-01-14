use super::HorseClient;
use crate::Build;
use anyhow::Context;
use anyhow::Result;
use russh::ChannelMsg;
use std::path::Path;
use tokio::io::AsyncWriteExt;

pub async fn run(sk: &Path, options: Build) -> Result<()> {
    let mut ssh = HorseClient::connect(
        sk,
        "cargo",
        std::env::var("HORSED").unwrap_or("127.0.0.1:2222".to_owned()),
    )
    .await?;

    let mut channel = ssh.channel_open_session().await?;
    channel.set_env(true, "REPO", "xu/workhorse").await?;
    channel.set_env(true, "BRANCH", "main").await?;
    channel
        .set_env(true, "CARGO_BUILD", serde_json::to_string(&options.cargo)?)
        .await?;
    channel.exec(true, "build").await?;

    let mut code = None;
    let mut stdout = tokio::io::stdout();

    loop {
        // There's an event available on the session channel
        let Some(msg) = channel.wait().await else {
            break;
        };
        match msg {
            // Write data to the terminal
            ChannelMsg::Data { ref data } => {
                stdout.write_all(data).await?;
                stdout.flush().await?;
            }
            // The command has returned an exit code
            ChannelMsg::ExitStatus { exit_status } => {
                code = Some(exit_status);
            }
            _ => {}
        }
    }

    ssh.close().await?;
    code.context("program did not exit cleanly")?;

    Ok(())
}
