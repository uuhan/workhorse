use anstyle::{AnsiColor, Effects};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
pub mod user;

pub use user::*;

pub fn styles() -> clap::builder::Styles {
    clap::builder::styling::Styles::styled()
        .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
        .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
        .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
        .placeholder(AnsiColor::Cyan.on_default())
        .error(AnsiColor::Red.on_default().effects(Effects::BOLD))
        .valid(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
        .invalid(AnsiColor::Yellow.on_default().effects(Effects::BOLD))
}

#[derive(Clone, Debug, Parser)]
#[clap(
    version,
    name = "horsed",
    styles = styles(),
    disable_help_subcommand = true,
)]
pub struct Cli {
    #[clap(short, long, help = "显示日志")]
    pub show_log: bool,

    #[clap(long, help = "!!! 维护服务常驻, 请注意使用风险 !!!")]
    pub dangerous: bool,

    #[clap(short, long = "fg", help = "前台运行")]
    pub foreground: bool,

    #[clap(short, long, help = "后台运行")]
    pub daemon: bool,

    #[clap(long, help = "指定工作目录")]
    pub dir: Option<PathBuf>,

    #[clap(subcommand)]
    pub commands: Option<Commands>,
}

#[derive(Clone, Debug, Subcommand)]
#[command(version, display_order = 1)]
pub enum Commands {
    #[command(name = "user", about = "账号管理")]
    User(User),
}
