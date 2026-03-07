use super::*;
use stable::data::v2::{self, Body};
use std::env;
use std::io;

#[cfg(unix)]
fn get_ulimit_n() -> Option<u64> {
    use std::mem;
    let mut limit: libc::rlimit = unsafe { mem::zeroed() };
    let ret = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) };
    if ret == 0 {
        Some(limit.rlim_cur as _)
    } else {
        None
    }
}

#[cfg(windows)]
fn get_ulimit_n() -> Option<u64> {
    None
}

fn default_shell() -> Option<String> {
    #[cfg(unix)]
    {
        env::var("SHELL").ok().filter(|s| !s.trim().is_empty())
    }
    #[cfg(windows)]
    {
        env::var("ComSpec")
            .ok()
            .or_else(|| env::var("COMSPEC").ok())
            .filter(|s| !s.trim().is_empty())
    }
}

fn horsed_commit() -> &'static str {
    option_env!("HORSED_GIT_SHA").unwrap_or("unknown")
}

impl AppServer {
    pub async fn health(&mut self, _args: Vec<String>) -> HorseResult<()> {
        let mut handle = self.handle.take().context("FIXME: NO HANDLE")?;
        let task = self.tm.spawn_handle();

        task.spawn(async move {
            let mut writer = handle.make_writer();
            let mut reader = handle.make_reader();

            let req = Body::read(&mut reader)
                .await
                .map_err(|e| anyhow::anyhow!("read health failed: {}", e))?;
            match req {
                Body::HealthCheck => {
                    let ulimit = get_ulimit_n();
                    let resp = Body::HealthStatus { ulimit };
                    let resp_bytes = bincode::serialize(&resp)?;

                    writer
                        .write_all(v2::head(resp_bytes.len() as _).as_bytes())
                        .await?;
                    writer.write_all(&resp_bytes).await?;
                }
                Body::HealthCheckV2 => {
                    let ulimit = get_ulimit_n();
                    let resp = Body::HealthStatusV2 {
                        ulimit,
                        version: env!("CARGO_PKG_VERSION").to_string(),
                        commit: horsed_commit().to_string(),
                        os: env::consts::OS.to_string(),
                        arch: env::consts::ARCH.to_string(),
                        family: env::consts::FAMILY.to_string(),
                        default_shell: default_shell(),
                    };
                    let resp_bytes = bincode::serialize(&resp)?;

                    writer
                        .write_all(v2::head(resp_bytes.len() as _).as_bytes())
                        .await?;
                    writer.write_all(&resp_bytes).await?;
                }
                _ => {
                    return Err(anyhow::anyhow!("协议错误, 期望 HealthCheck/HealthCheckV2"));
                }
            }

            writer.shutdown().await?;
            drop(writer);
            drop(reader);

            handle.eof().await?;
            handle.close().await?;

            Ok(())
        });

        Ok(())
    }
}
