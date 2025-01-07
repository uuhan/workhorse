use once_cell::sync::Lazy;
use rand_core::OsRng;
use russh_keys::PrivateKey;
use std::path::Path;

pub static KEY: Lazy<PrivateKey> = Lazy::new(|| {
    let key_file = Path::new("horsed.key");

    if key_file.exists() {
        tracing::info!("载入密钥文件: {:?}", key_file);
        russh_keys::PrivateKey::read_openssh_file(key_file).expect("无效的私钥文件")
    } else {
        tracing::info!("生成密钥文件: {:?}", key_file);
        let key = russh_keys::PrivateKey::random(&mut OsRng, ssh_key::Algorithm::Ed25519)
            .expect("无法生成私钥");

        #[cfg(windows)]
        key.write_openssh_file(Path::new(key_file), ssh_key::LineEnding::CRLF)
            .expect("无法写入私钥文件");
        #[cfg(not(windows))]
        key.write_openssh_file(Path::new(key_file), ssh_key::LineEnding::LF)
            .expect("无法写入私钥文件");

        key
    }
});
