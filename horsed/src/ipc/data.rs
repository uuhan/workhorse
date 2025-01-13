use serde::{Deserialize, Serialize};

/// Horsed Ipc 数据
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Data {
    /// Git Hooks 事件
    GitHook { kind: String, args: Vec<String> },
    /// 退出应用
    Exit,
}
