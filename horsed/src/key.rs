use rand_core::OsRng;
use russh::keys::{Algorithm, PrivateKey};
use std::path::Path;

const KEY_FILE: &str = "horsed.key";

pub fn key_exists() -> bool {
    Path::new(KEY_FILE).exists()
}

pub fn key_init() -> PrivateKey {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _lock = LOCK.lock().unwrap();

    let path = Path::new(KEY_FILE);

    if path.exists() {
        tracing::info!("载入密钥文件: {}", path.display());
        return PrivateKey::read_openssh_file(path).expect("无效的私钥文件");
    }

    tracing::info!("生成密钥文件: {}", path.display());
    let key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519).expect("无法生成私钥");

    #[cfg(windows)]
    key.write_openssh_file(path, ssh_key::LineEnding::CRLF)
        .expect("无法写入私钥文件");

    #[cfg(not(windows))]
    key.write_openssh_file(path, ssh_key::LineEnding::LF)
        .expect("无法写入私钥文件");

    key
}
