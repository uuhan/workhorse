pub use crate::mac::build::Build;
pub use crate::mac::check::Check;
pub use crate::mac::clean::Clean;
pub use crate::mac::clippy::Clippy;
pub use crate::mac::doc::Doc;
pub use crate::mac::install::Install;
pub use crate::mac::metadata::Metadata;
pub use crate::mac::run::Run;
pub use crate::mac::rustc::Rustc;
pub use crate::mac::test::Test;
pub use crate::mac::zigbuild::Zigbuild;
pub use crate::mac::CargoKind;
use clap::{Args, Parser, Subcommand};
use russh::keys::HashAlg;
use std::path::PathBuf;

/// 命令行参数
#[derive(Clone, Debug, Parser)]
#[clap(
    version,
    name = "cargo-work",
    styles = cargo_options::styles(),
    disable_help_subcommand = true,
)]
pub struct Cli {
    #[clap(short, long, help = "显示详细信息")]
    pub verbose: bool,

    #[clap(flatten)]
    pub horse: HorseOptions,

    #[clap(subcommand)]
    pub sub_commands: SubCommands,
}

#[derive(Default, Clone, Debug, Args)]
pub struct HorseOptions {
    #[clap(short, long = "ssh-key", help = "指定私钥文件路径")]
    pub key: Option<PathBuf>,
    #[clap(long = "key-hash-alg", help = "指定私钥文件哈希算法")]
    pub key_hash_alg: Option<HashAlg>,
    #[clap(
        long = "repo",
        help = "指定仓库地址, 例如: ssh://127.0.0.1:2222/uuhan/workhorse"
    )]
    pub repo: Option<String>,
    #[clap(long = "repo-name", help = "指定仓库名称, 例如: [/]uuhan/workhorse")]
    pub repo_name: Option<String>,
    #[clap(short, long = "remote", help = "指定 git remote 名称, 例如: horsed")]
    pub remote: Option<String>,
    #[clap(short, long, help = "指定脚本解释器")]
    pub shell: Option<String>,
    #[clap(short, long, help = "指定环境变量, e.g. --env KEY=VALUE")]
    pub env: Vec<String>,
    #[clap(short = 'x', long, help = "根据本地 ALL_PROXY 进行反向代理")]
    pub enable_proxy: bool,
    #[clap(long, help = "指定代理服务器")]
    pub all_proxy: Option<String>,
    #[clap(short = 't', long, help = "为命令运行分配一个 PTY")]
    pub pty: bool,
    #[clap(short, long, help = "检测代码变动")]
    pub watch: bool,
}

#[derive(Clone, Debug, Subcommand)]
#[command(version, display_order = 1)]
pub enum SubCommands {
    #[command(name = "work", alias = "cargo")]
    Work(WorkOptions),
    #[command(flatten)]
    Cargo(Commands),
}

#[derive(Clone, Debug, Parser)]
pub struct WorkOptions {
    #[clap(flatten)]
    pub horse: HorseOptions,

    #[clap(subcommand)]
    pub commands: Option<Commands>,

    pub scripts: Vec<String>,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Subcommand)]
#[command(version, display_order = 1)]
pub enum Commands {
    #[command(name = "init", about = "初始化项目")]
    Init(InitOption),

    #[command(name = "build", alias = "b", about = "构建项目")]
    Build(Build),
    #[command(name = "zigbuild", about = "使用 zigbuild 构建项目")]
    Zigbuild(Zigbuild),
    #[command(name = "check", alias = "c", about = "检查项目")]
    Check(Check),
    #[command(name = "clean", about = "清理工作目录")]
    Clean(Clean),
    #[command(name = "clippy", about = "检查项目")]
    Clippy(Clippy),
    #[command(name = "doc", about = "项目文档")]
    Doc(Doc),
    #[command(name = "install", alias = "i", about = "安装程序")]
    Install(Install),
    #[command(name = "metadata", about = "项目元数据")]
    Metadata(Metadata),
    #[command(name = "rustc", about = "编译器")]
    Rustc(Rustc),
    #[command(name = "test", alias = "t", about = "测试项目")]
    Test(Test),
    #[command(name = "run", alias = "r", about = "运行程序")]
    Run(Run),
    #[command(name = "just", alias = "j", about = "运行 just 任务")]
    Just(JustOptions),
    #[command(name = "get", alias = "g", about = "获取编译目录产物")]
    Get(GetOptions),
    #[command(name = "scp", alias = "cp", about = "拷贝服务器文件到本地, 类似于 scp")]
    Scp(ScpOptions),
    #[command(name = "push", about = "推送代码到远程仓库")]
    Push(PushOptions),
    #[command(name = "pull", about = "拉取代码到本地仓库")]
    Pull(PullOptions),
    #[command(name = "ping", about = "服务器状态检查")]
    Ping(PingOptions),
    #[command(name = "ssh", about = "连接服务器")]
    Ssh(SshOptions),
    #[command(name = "logs", about = "查看服务器日志")]
    Logs(LogsOptions),
    #[command(name = "watch", about = "监控文件变动并执行命令")]
    Watch(WatchOptions),

    #[command(name = "ra", about = "rust-analyzer 客户端")]
    RA(RaOptions),
}

#[derive(Clone, Debug, Args)]
pub struct InitOption {}

#[derive(Clone, Debug, Args)]
pub struct GetOptions {
    pub file: String,
    #[clap(short, long, help = "覆盖本地文件")]
    pub force: bool,
    #[clap(short, long, help = "输出到指定文件")]
    pub outfile: Option<PathBuf>,
    #[clap(short, long, help = "输出到标准输出")]
    pub stdout: bool,
    #[clap(flatten)]
    pub horse: HorseOptions,
}

#[derive(Clone, Debug, Args)]
pub struct ScpOptions {
    pub source: String,
    pub dest: String,
    #[clap(flatten)]
    pub horse: HorseOptions,
}

#[derive(Clone, Debug, Args)]
pub struct SshOptions {
    #[clap(flatten)]
    pub horse: HorseOptions,
    #[clap(
        short = 'L',
        name = "[LOCAL_IP:]LOCAL_PORT:DESTINATION:DESTINATION_PORT",
        help = "转发本地端口到远程端口"
    )]
    pub forward_local_port: Option<String>,
    #[clap(
        short = 'R',
        name = "[REMOTE_IP:]REMOTE_PORT:DESTINATION:DESTINATION_PORT",
        help = "转发远程端口到本地端口"
    )]
    pub forward_remote_port: Option<String>,
    // #[clap(
    //     short = 'D',
    //     name = "[LOCAL_IP:]LOCAL_PORT",
    //     help = "动态转发经由远程服务器"
    // )]
    // pub forward_dynamic_port: Option<String>,
    pub commands: Vec<String>,
}

#[derive(Clone, Debug, Args)]
pub struct LogsOptions {
    #[clap(flatten)]
    pub horse: HorseOptions,
    #[clap(short, help = "持续获取日志")]
    pub forward: bool,
}

#[derive(Clone, Debug, Args)]
pub struct WatchOptions {
    #[clap(flatten)]
    pub horse: HorseOptions,
    #[clap(short, long, help = "检测任意文件变动")]
    pub any: bool,
    #[clap(short, long, help = "指定目录路径")]
    pub path: Option<PathBuf>,
    pub commands: Vec<String>,
}

#[derive(Clone, Debug, Args)]
pub struct PingOptions {
    #[clap(flatten)]
    pub horse: HorseOptions,
    #[clap(short, long, help = "指定次数")]
    pub count: Option<u32>,
    pub remote: Option<String>,
}

#[derive(Clone, Debug, Args)]
pub struct PushOptions {
    #[clap(help = "远程仓库地址, 默认: horsed")]
    pub remote: Option<String>,
    #[clap(help = "推送的分支, 默认为当前分支")]
    pub branch: Option<String>,
}

#[derive(Clone, Debug, Args)]
pub struct PullOptions {
    #[clap(flatten)]
    pub horse: HorseOptions,
    pub branch: Option<String>,
}

#[derive(Clone, Debug, Args)]
pub struct JustOptions {
    #[clap(short, long, help = "指定配置文件")]
    pub file: Option<String>,
    pub command: Vec<String>,
    #[clap(flatten)]
    pub horse: HorseOptions,
}

#[derive(Clone, Debug, Args)]
pub struct RaOptions {
    #[command(subcommand)]
    pub command: Option<RaCommand>,
}

#[derive(Clone, Debug, Subcommand)]
#[command(version, display_order = 1)]
pub enum RaCommand {
    /// 连接到代理(默认)
    Client(RaClientOptions),
    /// 代理配置
    Config(RaConfigOptions),
    /// 代理状态
    Status(RaStatusOptions),
    /// 更新工作区内容
    Reload(RaReloadOptions),
}

#[derive(Clone, Debug, Args)]
pub struct RaClientOptions {}

#[derive(Clone, Debug, Args)]
pub struct RaConfigOptions {}

#[derive(Clone, Debug, Args)]
pub struct RaReloadOptions {}

#[derive(Clone, Debug, Args)]
pub struct RaStatusOptions {
    #[clap(long = "json", short = 'j', default_value = "false")]
    pub json: bool,
}
