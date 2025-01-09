use crate::prelude::*;
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub struct Repo {
    dir: PathBuf,
}

impl Repo {
    pub fn from(path: impl AsRef<Path>) -> Self {
        // TODO 仓库配置
        Repo {
            dir: path.as_ref().to_path_buf(),
        }
    }

    pub fn path(&self) -> &Path {
        self.dir.as_ref()
    }

    pub fn exists(&self) -> bool {
        self.dir.exists()
    }

    pub async fn init_bare(&mut self) -> HorseResult<()> {
        let _ = Repo::create_bare(&self.dir).await?;
        Ok(())
    }

    /// 创建一个裸仓库, 用于存放代码
    pub async fn create_bare(path: impl AsRef<Path>) -> HorseResult<Repo> {
        tracing::debug!("CREATE BARE REPO: {}", path.as_ref().display());
        Command::new("git")
            .arg("init")
            .arg("--bare")
            .arg(path.as_ref())
            .output()
            .await?
            .status
            .exit_ok()?;

        // Command::new("git")
        //     .current_dir(path.as_ref())
        //     .arg("branch")
        //     .arg("-m")
        //     .arg("master")
        //     .output()
        //     .await?
        //     .status
        //     .exit_ok()?;

        let dir = path.as_ref().to_path_buf();
        Ok(Repo { dir })
    }

    /// 从远程仓库克隆代码
    pub async fn clone(
        from: impl AsRef<Path>,
        to: impl AsRef<Path>,
        branch: Option<&str>,
    ) -> HorseResult<Self> {
        Command::new("git")
            .arg("clone")
            .arg("--branch")
            .arg(branch.unwrap_or("master"))
            .arg(from.as_ref().to_str().unwrap())
            .arg(to.as_ref().to_str().unwrap())
            .output()
            .await?
            .status
            .exit_ok()?;

        Ok(Repo::from(to))
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
