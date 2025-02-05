use std::time::Instant;

use super::*;
use std::io::{self, Read};
use tokio::io::{AsyncRead, AsyncReadExt};
pub use v1::{GetFile, GetKind};

#[derive(Serialize, Deserialize, Debug)]
#[non_exhaustive]
pub enum Body {
    GetFile(GetFile),
    #[serde(with = "instant_serde")]
    Ping(Instant),
    #[serde(with = "instant_serde")]
    Pong(Instant),
}

pub fn head(size: u16) -> Head {
    Head { version: 2, size }
}

impl Head {
    pub fn v2(&self) -> bool {
        self.version == 1
    }
}

impl Body {
    pub async fn read<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Self> {
        let mut head = [0u8; HEAD_SIZE];
        reader.read_exact(&mut head).await?;
        let head = Head::ref_from_bytes(&head).unwrap();

        let mut body = vec![0u8; head.size as usize];
        reader.read_exact(&mut body).await?;
        Ok(bincode::deserialize::<Body>(&body).expect("malformed body"))
    }

    pub fn read_sync<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut head = [0u8; HEAD_SIZE];
        reader.read_exact(&mut head)?;
        let head = Head::ref_from_bytes(&head).unwrap();

        let mut body = vec![0u8; head.size as usize];
        reader.read_exact(&mut body)?;
        Ok(bincode::deserialize::<Body>(&body).expect("malformed body"))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_header_size() {
        assert_eq!(HEAD_SIZE, 3);
    }

    #[test]
    fn test_get_file_body() {
        let file = GetFile {
            path: PathBuf::from("/foo/bar/"),
            kind: GetKind::File,
            size: None,
        };
        assert_eq!(bincode::serialize(&file).unwrap().len(), 22);
        let body = Body::GetFile(file);
        assert_eq!(bincode::serialize(&body).unwrap().len(), 26);
    }
}
