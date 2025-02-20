use super::*;
use crate::options::SshOptions;
use color_eyre::eyre::{anyhow, ContextCompat, Result, WrapErr};
use git2::Repository;
use std::path::Path;
use tokio::net::TcpListener;

pub async fn run(sk: &Path, options: SshOptions) -> Result<()> {
    let repo = Repository::discover(".")?;
    let head = repo.head()?;

    let repo_name = if let Some(repo_name) = find_repo_name(&options.horse) {
        repo_name
    } else {
        // 无法从参数获取 repo_name, 尝试从 git remote 获取
        // 默认远程仓库为 horsed,
        // 格式: ssh://git@192.168.10.62:2222/<ns>/<repo_name>
        let Some(horsed) = find_remote(&repo, &options.horse) else {
            return Err(anyhow!("找不到 horsed 远程仓库!"));
        };

        horsed
            .url()
            .and_then(extract_repo_name)
            .context("获取 horsed 远程仓库 URL 失败")?
    };

    let host = if let Ok(host) = std::env::var("HORSED") {
        host.parse()?
    } else if let Some(host) = find_host(&options.horse) {
        host
    } else {
        let Some(horsed) = find_remote(&repo, &options.horse) else {
            return Err(anyhow!("找不到 horsed 远程仓库!"));
        };

        horsed
            .url()
            .and_then(extract_host)
            .context("获取 horsed 远程仓库 HOST 失败")?
    };

    let branch = head
        .shorthand()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "master".to_owned());

    if let Some(forward_local_port) = options.forward_local_port.clone() {
        connect_forward_l(sk, host, forward_local_port, &options).await
    } else if let Some(forward_remote_port) = options.forward_remote_port.clone() {
        connect_forward_r(sk, host, forward_remote_port, &options).await
    } else {
        connect_shell(sk, host, repo_name, branch, &options).await
    }
}

/// ssh -L
pub async fn connect_forward_l(
    sk: &Path,
    host: impl ToSocketAddrs,
    forward_local_port: impl AsRef<str>,
    options: &SshOptions,
) -> Result<()> {
    let mut ssh =
        HorseClient::connect(sk, options.horse.key_hash_alg, "ssh", host, None, None).await?;

    let mut addrs = forward_local_port
        .as_ref()
        .split(":")
        .collect::<Vec<&str>>();
    addrs.reverse();
    let remote_port = addrs.first().unwrap().parse::<u32>()?;
    let remote_host = addrs.get(1).unwrap().parse::<String>()?;
    let local_port = addrs.get(2).unwrap().parse::<u32>()?;
    let local_host = addrs
        .get(3)
        .and_then(|s| s.parse::<String>().ok())
        .unwrap_or("127.0.0.1".to_string());

    let local_addr = format!("{}:{}", local_host, local_port);
    println!("Listening on {}", local_addr);
    let listener = TcpListener::bind(&local_addr).await?;

    while let Ok((mut stream, addr)) = listener.accept().await {
        println!(
            "{:?} -> {} -> {}:{}",
            addr, local_addr, remote_host, remote_port
        );

        let channel = match ssh
            .channel_open_direct_tcpip(&remote_host, remote_port, &local_host, local_port)
            .await
            .wrap_err("tcp forward failed!")
        {
            Ok(channel) => channel,
            Err(e) => {
                eprintln!("tcpip forward failed: {:?}", e);
                ssh.close().await?;
                return Ok(());
            }
        };

        let mut ch_stream = channel.into_stream();
        tokio::io::copy_bidirectional(&mut ch_stream, &mut stream).await?;
    }

    Ok(())
}

/// ssh -R
pub async fn connect_forward_r(
    sk: &Path,
    host: impl ToSocketAddrs,
    forward_remote_port: impl AsRef<str>,
    options: &SshOptions,
) -> Result<()> {
    let mut addrs = forward_remote_port
        .as_ref()
        .split(":")
        .collect::<Vec<&str>>();
    addrs.reverse();
    let local_port = addrs
        .first()
        .unwrap()
        .parse::<u32>()
        .context("port parse")?;
    let local_host = addrs
        .get(1)
        .unwrap()
        .parse::<String>()
        .context("host parse")?;
    let remote_port = addrs.get(2).unwrap().parse::<u32>().context("port parse")?;
    let remote_host = addrs
        .get(3)
        .and_then(|s| s.parse::<String>().ok())
        .unwrap_or("127.0.0.1".to_string());

    let mut ssh = HorseClient::connect(
        sk,
        options.horse.key_hash_alg,
        "ssh",
        host,
        Some(local_host),
        Some(local_port),
    )
    .await?;

    ssh.tcpip_forward(&remote_host, remote_port).await?;
    println!("(Remote) Listening on {}:{}", remote_host, remote_port);

    let mut channel = ssh
        .channel_open_session()
        .await
        .with_context(|| "channel_open_session error.")?;

    while let Some(msg) = channel.wait().await {
        println!("{:?}", msg);
    }

    Ok(())
}

/// default shell
pub async fn connect_shell(
    sk: &Path,
    host: impl ToSocketAddrs,
    repo_name: String,
    branch: String,
    options: &SshOptions,
) -> Result<()> {
    let mut ssh =
        HorseClient::connect(sk, options.horse.key_hash_alg, "ssh", host, None, None).await?;

    let channel = ssh.channel_open_session().await?;
    channel.set_env(true, "REPO", repo_name).await?;
    channel.set_env(true, "BRANCH", branch).await?;
    for kv in options.horse.env.iter() {
        let (k, v) = kv.split_once('=').unwrap_or_else(|| (kv, ""));
        channel.set_env(true, k, v).await?;
    }

    crossterm::terminal::enable_raw_mode()?;

    let code = { ssh.shell(&options.commands.join(" ")).await? };

    crossterm::terminal::disable_raw_mode()?;

    println!("exit code: {}", code);
    ssh.close().await?;

    Ok(())
}

pub async fn start_proxy(
    sk: &Path,
    host: SocketAddr,
    options: &HorseOptions,
) -> Result<Vec<String>> {
    let mut env = options.env.clone();

    // --all-proxy=socks://IP:PORT
    let (enable_proxy, proxy) = if let Some(proxy) = options.all_proxy.clone() {
        (true, proxy)
    } else if options.enable_proxy {
        if let Ok(proxy) = std::env::var("ALL_PROXY").or(std::env::var("all_proxy")) {
            (true, proxy)
        } else {
            println!("未设置代理, 请设置环境变量 ALL_PROXY 或 all_proxy");
            return Ok(env);
        }
    } else {
        (false, "".to_owned())
    };

    // proxy enabled
    if enable_proxy {
        use rand::Rng;
        use url::Url;
        let proxy = Url::parse(&proxy)?;
        let mut rng = rand::thread_rng();
        let random_port = rng.gen_range(3000..10000);
        let forward = format!(
            "{}:{}:{}",
            random_port,
            proxy.host().expect("proxy host missing"),
            proxy.port().expect("proxy port missing")
        );
        let sk_ = std::path::PathBuf::from(sk);
        let ssh_options = crate::options::SshOptions {
            horse: options.clone(),
            forward_local_port: None,
            forward_remote_port: Some(forward.clone()),
            commands: vec![],
        };

        let proxy_scheme = proxy.scheme();
        env.push(format!(
            "ALL_PROXY={proxy_scheme}://127.0.0.1:{random_port}"
        ));
        env.push(format!("HTTP_PROXY=http://127.0.0.1:{random_port}"));
        env.push(format!("HTTPS_PROXY=http://127.0.0.1:{random_port}"));

        tokio::spawn(async move {
            super::ssh::connect_forward_r(&sk_, host, forward, &ssh_options).await?;
            Ok::<_, color_eyre::Report>(())
        });
    }

    Ok(env)
}
