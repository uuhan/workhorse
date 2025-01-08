use crate::prelude::HorseResult;
use russh::{
    server::{Handle, Msg, Session},
    Channel, ChannelId,
};
use std::process::ExitStatus;
use std::process::Stdio;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::process::Command;

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

    pub fn make_io_pair<'a>(&'a mut self) -> (impl AsyncWrite, impl AsyncRead + 'a) {
        (self.ch.make_writer(), self.ch.make_reader())
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
    pub async fn exit(self, exit_status: ExitStatus) -> HorseResult<()> {
        let _ = self
            .handle
            .exit_status_request(self.id, exit_status.code().unwrap_or(128) as _)
            .await;
        self.finish().await?;
        Ok(())
    }

    /// 结束通道，关闭通道和发送EOF
    pub async fn finish(mut self) -> HorseResult<()> {
        self.eof().await?;
        self.close().await?;
        Ok(())
    }

    /// 调用远程命令, 并将输入输出流通过通道传输
    pub async fn exec(mut self, cmd: &mut Command) -> HorseResult<()> {
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        let mut cmd = cmd.spawn()?;

        let mut stdout = cmd.stdout.take().unwrap();
        let mut stdin = cmd.stdin.take().unwrap();
        let (mut cout, mut cin) = self.make_io_pair();

        let mut i_ready = false;
        let mut o_ready = false;
        loop {
            if i_ready && o_ready {
                break;
            }
            tokio::select! {
                i = tokio::io::copy(&mut cin, &mut stdin) => {
                    match i {
                        Ok(len) => {
                            tracing::debug!("receive data: {}", len);
                            if len == 0 {
                                i_ready = true;
                            }
                        },
                        Err(e) => {
                            tracing::error!("receive data error: {}", e);
                            break;
                        }
                    }
                },
                o = tokio::io::copy(&mut stdout, &mut cout) => {
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
            }
        }

        drop((cout, cin));
        self.exit(cmd.wait().await?).await?;

        Ok(())
    }
}
