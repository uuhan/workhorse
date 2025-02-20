use crate::options::PushOptions;
use color_eyre::eyre::Result;
use std::path::Path;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub async fn run(sk: &Path, options: PushOptions) -> Result<()> {
    // let repo = git2::Repository::discover(".")?;
    // let head = repo.head()?;

    let mut cmd = Command::new("git");
    cmd.kill_on_drop(true);
    cmd.arg("push")
        .arg(options.remote.unwrap_or("horsed".into()));

    if let Some(ref branch) = options.branch {
        cmd.arg(branch);
    }

    let output = cmd.output().await?;

    tokio::io::stdout().write_all(&output.stdout).await?;
    tokio::io::stderr().write_all(&output.stderr).await?;

    Ok(())
}
