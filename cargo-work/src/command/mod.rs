#![allow(unused_variables)]
use crate::options::HorseOptions;
use color_eyre::eyre::{bail, Result};
use colored::Colorize;
use git2::Remote;
use git2::Repository;
use russh::client::Msg;
use russh::client::{self, DisconnectReason, Handle, Handler};
use russh::keys::key::PrivateKeyWithHashAlg;
use russh::keys::ssh_key::PublicKey;
use russh::keys::*;
use russh::*;
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpSocket;
use tokio::net::ToSocketAddrs;
use url::Url;

pub mod cargo;
pub mod cmd;
pub mod get;
pub mod just;
pub mod ping;
pub mod pull;
pub mod push;
pub mod scp;
pub mod ssh;
pub mod watch;

pub struct HorseClient {
    handle: Handle<Client>,
}

pub struct Client {
    pub forward_host: Option<String>,
    pub forward_port: Option<u32>,
}

#[async_trait::async_trait]
impl Handler for Client {
    type Error = russh::Error;

    async fn check_server_key(&mut self, _pk: &PublicKey) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn auth_banner(
        &mut self,
        banner: &str,
        _session: &mut russh::client::Session,
    ) -> Result<(), Self::Error> {
        for banner in banner.lines() {
            println!(
                "{}{}{} {}",
                "[".bold(),
                "HORSED".green(),
                "]".bold(),
                banner.yellow()
            );
        }
        Ok(())
    }

    /// Called when the server sent a disconnect message
    ///
    /// If reason is an Error, this function should re-return the error so the join can also evaluate it
    async fn disconnected(
        &mut self,
        reason: DisconnectReason<Self::Error>,
    ) -> Result<(), Self::Error> {
        match reason {
            DisconnectReason::ReceivedDisconnect(_) => Ok(()),
            DisconnectReason::Error(e) => Err(e),
        }
    }

    /// Called when the server sends us data. The `extended_code`
    /// parameter is a stream identifier, `None` is usually the
    /// standard output, and `Some(1)` is the standard error. See
    /// [RFC4254](https://tools.ietf.org/html/rfc4254#section-5.2).
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called when the server sends us data. The `extended_code`
    /// parameter is a stream identifier, `None` is usually the
    /// standard output, and `Some(1)` is the standard error. See
    /// [RFC4254](https://tools.ietf.org/html/rfc4254#section-5.2).
    async fn extended_data(
        &mut self,
        channel: ChannelId,
        ext: u32,
        data: &[u8],
        session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Called when the server opens a channel for a new remote port forwarding connection
    #[allow(unused_variables)]
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        connected_address: &str,
        connected_port: u32,
        originator_address: &str,
        originator_port: u32,
        session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let host = self
            .forward_host
            .as_ref()
            .map_or(connected_address, |v| v.as_str());
        let port = self.forward_port.unwrap_or(connected_port);

        println!(
            "{}:{} <- {}:{} <- {}:{}",
            host, port, connected_address, connected_port, originator_address, originator_port
        );

        let socket = TcpSocket::new_v4()?;
        let Ok(mut stream) = socket
            .connect(format!("{}:{}", host, port).parse().unwrap())
            .await
        else {
            session.disconnect(Disconnect::ByApplication, "", "English")?;
            return Ok(());
        };

        tokio::spawn(async move {
            let mut ch_stream = channel.into_stream();
            tokio::io::copy_bidirectional(&mut ch_stream, &mut stream)
                .await
                .unwrap();

            Ok::<_, Self::Error>(())
        });

        Ok(())
    }

    #[allow(unused_variables)]
    async fn channel_open_confirmation(
        &mut self,
        channel: ChannelId,
        max_packet_size: u32,
        window_size: u32,
        session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        // println!(
        //     "channel open: {:?} {} {}",
        //     channel, max_packet_size, window_size
        // );
        Ok(())
    }
}

impl HorseClient {
    #[allow(unused)]
    async fn connect<P: AsRef<Path>, A: ToSocketAddrs>(
        key_path: P,
        key_hash_alg: Option<HashAlg>,
        user: impl Into<String>,
        addrs: A,
        forward_host: Option<String>,
        forward_port: Option<u32>,
    ) -> Result<Self> {
        let key_pair = load_secret_key(key_path, None)?;
        let config = client::Config {
            inactivity_timeout: None,
            keepalive_interval: Some(Duration::from_secs(3)),
            ..<_>::default()
        };

        let config = Arc::new(config);
        let sh = Client {
            forward_host,
            forward_port,
        };

        let mut handle = client::connect(config, addrs, sh).await?;
        let auth_res = handle
            .authenticate_publickey(
                user,
                PrivateKeyWithHashAlg::new(
                    Arc::new(key_pair),
                    key_hash_alg.or(Default::default()),
                )?,
            )
            .await?;

        if !auth_res {
            bail!("Authentication failed");
        }

        Ok(Self { handle })
    }

    #[allow(unused)]
    async fn close(&mut self) -> Result<()> {
        self.handle
            .disconnect(Disconnect::ByApplication, "", "English")
            .await?;
        Ok(())
    }
}

impl Deref for HorseClient {
    type Target = Handle<Client>;
    fn deref(&self) -> &Self::Target {
        &self.handle
    }
}

impl DerefMut for HorseClient {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.handle
    }
}

/// 获取默认的配置, 目前 `牛马` 设置远程仓库名为 `horsed` 或 `just-horsed`
fn find_remote<'a>(repo: &'a Repository, options: &'a HorseOptions) -> Option<Remote<'a>> {
    // --remote <REMOTE>
    if let Some(ref remote) = options.remote {
        return repo.find_remote(remote).ok();
    }

    if let Ok(remote) = repo.find_remote("horsed") {
        return Some(remote);
    }

    None
}

fn find_repo_name(options: &HorseOptions) -> Option<String> {
    // 如果参数中指定了远程仓库, 则使用参数指定的仓库
    if let Some(ref repo_name) = options.repo_name {
        return Some(repo_name.to_string());
    }

    // 如果参数中指定了远程仓库的 URL, 则使用参数指定的 URL
    if let Some(ref url) = options.repo {
        return extract_repo_name(url);
    }

    None
}

fn find_host(options: &HorseOptions) -> Option<SocketAddr> {
    options.repo.as_ref().and_then(|s| extract_host(s))
}

fn extract_host(url: &str) -> Option<SocketAddr> {
    let url = Url::parse(url).ok()?;

    url.socket_addrs(|| Some(2222))
        .ok()
        .and_then(|addrs| addrs.first().copied())
}

fn extract_repo_name(url: &str) -> Option<String> {
    let url = Url::parse(url).ok()?;
    url.path().strip_prefix("/").map(|s| s.to_string())
}

#[cfg(feature = "use-system-ssh")]
fn run_system_ssh<K, V, Arg, Args>(
    key: &Path,
    env: &[(K, V)],
    action: &str,
    host: SocketAddr,
    args: Args,
) -> tokio::process::Command
where
    K: AsRef<str>,
    V: AsRef<str>,
    Arg: AsRef<std::ffi::OsStr>,
    Args: std::iter::IntoIterator<Item = Arg>,
{
    let mut cmd = tokio::process::Command::new("ssh");

    #[cfg(target_os = "windows")]
    {
        #[allow(unused_imports)]
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    cmd.arg("-i");
    cmd.arg(key);
    cmd.arg("-o");
    cmd.arg(format!(
        "SetEnv {}",
        env.iter()
            .map(|(k, v)| format!("{}={}", k.as_ref(), v.as_ref()))
            .collect::<Vec<_>>()
            .join(" ")
    ));
    cmd.arg(format!("{}@{}", action, host.ip()));
    cmd.arg("-p");
    cmd.arg(format!("{}", host.port()));
    cmd.args(args);

    cmd
}
