use clap::{Args, Parser, Subcommand};

#[derive(Clone, Debug, Parser)]
#[command(version, display_order = 1)]
pub struct User {
    #[clap(subcommand)]
    pub commands: UserCommand,
}

#[derive(Clone, Debug, Subcommand)]
pub enum UserCommand {
    #[command(name = "add", about = "添加用户")]
    Add(AddUser),
    #[command(name = "del", about = "删除用户")]
    Del(DelUser),
    #[command(name = "mod", about = "修改用户")]
    Mod(ModUser),
    #[command(name = "list", about = "列出用户")]
    List(ListUser),
}

#[derive(Clone, Debug, Parser)]
pub struct AddUser {
    #[clap(short, long, help = "用户名")]
    pub name: String,
    #[clap(long, help = "用户昵称")]
    pub nick: Option<String>,
    #[clap(short, long, help = "用户邮箱")]
    pub email: Option<String>,
    #[clap(short, long, help = "用户密钥", value_parser = parse_user_key)]
    pub key: Option<UserKey>,
}

fn parse_user_key(pk: &str) -> Result<UserKey, String> {
    // ssh-ed25519 XXX ""
    let parts: Vec<String> = pk.split(' ').map(String::from).collect();
    let mut parts = parts.into_iter();
    let Some(method) = parts.next() else {
        return Err(pk.to_string());
    };
    let Some(key) = parts.next() else {
        return Err(pk.to_string());
    };
    let comment = parts.next();

    Ok(UserKey {
        method,
        key,
        comment,
    })
}

#[derive(Debug, Clone)]
pub struct UserKey {
    /// 密钥类型
    pub method: String,
    /// 密钥字串
    pub key: String,
    /// 密钥备注
    pub comment: Option<String>,
}

#[derive(Clone, Debug, Parser)]
pub struct DelUser {}

#[derive(Clone, Debug, Parser)]
pub struct ModUser {}

#[derive(Clone, Debug, Parser)]
pub struct ListUser {}
