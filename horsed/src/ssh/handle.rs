use crate::prelude::HorseResult;
use colored::{ColoredString, Colorize};
use russh::{
    server::{Handle, Msg, Session},
    Channel, ChannelId, CryptoVec,
};
use std::ops::{Deref, DerefMut};
use std::process::ExitStatus;
use std::process::Stdio;
use tokio::process::Command;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    process::Child,
};

pub struct ChannelHandle {
    pub(crate) handle: Handle,
    pub(crate) id: ChannelId,
    pub(crate) ch: Channel<Msg>,
}

impl ChannelHandle {
    pub fn from(channel: Channel<Msg>, session: &mut Session) -> Self {
        Self {
            handle: session.handle(),
            id: channel.id(),
            ch: channel,
        }
    }

    pub fn make_io_pair(&mut self) -> (impl AsyncWrite, impl AsyncRead + '_) {
        (self.make_writer(), self.make_reader())
    }

    pub fn make_writer(&self) -> impl AsyncWrite {
        self.ch.make_writer()
    }

    pub fn make_reader(&mut self) -> impl AsyncRead + '_ {
        self.ch.make_reader()
    }

    pub async fn eof(&mut self) -> HorseResult<()> {
        let _ = self.handle.eof(self.id).await;
        Ok(())
    }

    pub async fn close(&mut self) -> HorseResult<()> {
        let _ = self.handle.close(self.id).await;
        Ok(())
    }

    /// `exec_request`, 发送请求状态，并结束通道
    pub async fn exit(&self, exit_status: ExitStatus) -> HorseResult<()> {
        self.handle
            .exit_status_request(self.id, exit_status.code().unwrap_or(128) as _)
            .await
            .map_err(|_| anyhow::anyhow!("EXIT STATUS REQUEST").into())
    }

    /// 调用远程命令, 并将输入输出流通过通道传输
    pub async fn exec(&mut self, cmd: &mut Command) -> HorseResult<Child> {
        #[cfg(target_os = "windows")]
        {
            #[allow(unused_imports)]
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;

            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut cmd = cmd.spawn()?;

        let mut stdout = cmd.stdout.take().unwrap();
        let mut stderr = cmd.stderr.take().unwrap();

        let mut eout = self.make_writer();
        let mut cout = self.make_writer();

        let mut o_ready = false;
        let mut e_ready = false;
        loop {
            if o_ready && e_ready {
                break;
            }
            tokio::select! {
                o = tokio::io::copy(&mut stdout, &mut cout), if !o_ready => {
                    match o {
                        Ok(len) => {
                            tracing::debug!("send data: {}", len);
                            if len == 0 {
                                o_ready = true;
                            }
                        },
                        Err(e) => {
                            tracing::error!("send data error: {}", e);
                            break;
                        }
                    }
                },
                e = tokio::io::copy(&mut stderr, &mut eout), if !e_ready => {
                    match e {
                        Ok(len) => {
                            if len == 0 {
                                e_ready = true;
                            }
                        },
                        Err(e) => {
                            tracing::error!("send data error: {}", e);
                            break;
                        }
                    }
                },
                else => {
                    break;
                }
            }
        }

        Ok(cmd)
    }

    /// 调用远程命令, 并将输入输出流通过通道传输
    pub async fn exec_io(&mut self, cmd: &mut Command) -> HorseResult<Child> {
        #[cfg(target_os = "windows")]
        {
            #[allow(unused_imports)]
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;

            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut cmd = cmd.spawn()?;

        let mut stdin = cmd.stdin.take().unwrap();
        let mut stdout = cmd.stdout.take().unwrap();
        let mut stderr = cmd.stderr.take().unwrap();

        let mut eout = self.make_writer();
        let (mut cout, mut cin) = self.make_io_pair();

        let mut i_ready = false;
        let mut o_ready = false;
        let mut e_ready = false;
        loop {
            tokio::select! {
                i = tokio::io::copy(&mut cin, &mut stdin), if !i_ready => {
                    match i {
                        Ok(len) => {
                            tracing::debug!("send data: {}", len);
                            if len == 0 {
                                i_ready = true;
                            }
                        },
                        Err(e) => {
                            tracing::error!("send data error: {}", e);
                            break;
                        }
                    }
                },
                o = tokio::io::copy(&mut stdout, &mut cout), if !o_ready => {
                    match o {
                        Ok(len) => {
                            tracing::debug!("send data: {}", len);
                            if len == 0 {
                                o_ready = true;
                            }
                        },
                        Err(e) => {
                            tracing::error!("send data error: {}", e);
                            break;
                        }
                    }
                },
                e = tokio::io::copy(&mut stderr, &mut eout), if !e_ready => {
                    match e {
                        Ok(len) => {
                            if len == 0 {
                                e_ready = true;
                            }
                        },
                        Err(e) => {
                            tracing::error!("send data error: {}", e);
                            break;
                        }
                    }
                },
                else => {
                    break;
                }
            }
        }

        Ok(cmd)
    }

    #[allow(unused)]
    pub async fn info(&self, text: impl AsRef<str>) -> HorseResult<()> {
        self.log("HORSED".green(), text).await
    }

    #[allow(unused)]
    pub async fn warn(&self, text: impl AsRef<str>) -> HorseResult<()> {
        self.log("HORSED".yellow(), text).await
    }

    #[allow(unused)]
    pub async fn error(&self, text: impl AsRef<str>) -> HorseResult<()> {
        self.log("HORSED".red(), text).await
    }

    /// 发送消息告知客户端
    /// 使用 SSH 协议的扩展数据传输: SSH_EXTENDED_DATA_STDERR = 1
    /// 参考: https://datatracker.ietf.org/doc/html/rfc4254#section-5.2
    async fn log(&self, title: ColoredString, text: impl AsRef<str>) -> HorseResult<()> {
        let msg = format!(
            "{}{}{} {}\n",
            "[".bold(),
            title.bold(),
            "]".bold(),
            text.as_ref(),
        );
        let msg = CryptoVec::from(msg);
        if let Err(vec) = self.handle.extended_data(self.id, 1, msg).await {
            return Err(anyhow::anyhow!("SEND MESSAGE: {:?}", vec))?;
        }
        Ok(())
    }

    /// 类似 `self.log` 方法, 但是输出 &[u8] 数据到前端
    pub async fn log_raw(&self, raw: impl AsRef<[u8]>) -> HorseResult<()> {
        let raw = CryptoVec::from(raw.as_ref());
        if let Err(vec) = self.handle.extended_data(self.id, 1, raw).await {
            return Err(anyhow::anyhow!("SEND MESSAGE: {:?}", vec))?;
        }

        Ok(())
    }

    /// 发送扩展数据到前端
    #[allow(dead_code)]
    pub async fn extended_data(&self, ext: u32, data: impl AsRef<[u8]>) -> HorseResult<()> {
        let raw = CryptoVec::from(data.as_ref());
        if let Err(vec) = self.handle.extended_data(self.id, ext, raw).await {
            return Err(anyhow::anyhow!(
                "SEND EXT DATA: ext: {}, len: {}",
                ext,
                vec.len()
            ))?;
        }

        Ok(())
    }
}

impl Deref for ChannelHandle {
    type Target = Handle;
    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

impl DerefMut for ChannelHandle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.handle
    }
}

impl Drop for ChannelHandle {
    fn drop(&mut self) {
        tracing::debug!("channel {} dropped", self.id);
        futures::executor::block_on(async move {
            let _ = self.eof().await;
            let _ = self.close().await;
        });
    }
}
