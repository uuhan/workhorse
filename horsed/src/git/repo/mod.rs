use crate::prelude::*;
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub struct Repo {
    dir: PathBuf,
}

impl Repo {
    pub async fn from(path: impl AsRef<Path>) -> HorseResult<Self> {
        // TODO 仓库配置
        Ok(Repo {
            dir: path.as_ref().to_path_buf(),
        })
    }

    pub async fn create_bare(path: impl AsRef<Path>) -> HorseResult<Repo> {
        Command::new("git")
            .arg("init")
            .arg("--bare")
            .current_dir(path.as_ref())
            .output()
            .await?
            .status
            .exit_ok()?;

        Command::new("git")
            .current_dir(path.as_ref())
            .arg("branch")
            .arg("-m")
            .arg("master")
            .output()
            .await?
            .status
            .exit_ok()?;

        let dir = path.as_ref().to_path_buf();
        Ok(Repo { dir })
    }

    pub async fn clone(from: impl AsRef<Path>, to: impl AsRef<Path>) -> HorseResult<Self> {
        Command::new("git")
            .arg("clone")
            .arg(from.as_ref().to_str().unwrap())
            .arg(to.as_ref().to_str().unwrap())
            .output()
            .await?
            .status
            .exit_ok()?;

        Repo::from(to).await
    }

    pub async fn push_changes(&self, message: impl AsRef<str>) -> HorseResult<()> {
        Command::new("git")
            .current_dir(&self.dir)
            .arg("add")
            .arg(".")
            .output()
            .await?
            .status
            .exit_ok()?;

        Command::new("git")
            .current_dir(&self.dir)
            .arg("commit")
            .arg("-m")
            .arg(message.as_ref())
            .output()
            .await?
            .status
            .exit_ok()?;

        Command::new("git")
            .current_dir(&self.dir)
            .arg("push")
            .output()
            .await?
            .status
            .exit_ok()?;

        Ok(())
    }
}
