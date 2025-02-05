use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::PathBuf;
use tokio::io::{AsyncRead, AsyncReadExt};

pub use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};
pub mod v1;
pub mod v2;

pub const HEAD_SIZE: usize = std::mem::size_of::<Head>();

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Clone, Debug)]
#[repr(packed)]
pub struct Head {
    pub version: u8,
    pub size: u16,
}

impl Head {
    pub async fn read<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Self> {
        let mut head = [0u8; HEAD_SIZE];
        reader.read_exact(&mut head).await?;
        Ok(Head::read_from_bytes(&head).expect("malformed head"))
    }

    pub fn read_sync<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut head = [0u8; HEAD_SIZE];
        reader.read_exact(&mut head)?;
        Ok(Head::read_from_bytes(&head).expect("malformed head"))
    }
}

mod instant_serde {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::{Duration, Instant};

    pub fn serialize<S>(instant: &Instant, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let duration = instant.elapsed();
        duration.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Instant, D::Error>
    where
        D: Deserializer<'de>,
    {
        let duration = Duration::deserialize(deserializer)?;
        let now = Instant::now();
        let instant = now
            .checked_sub(duration)
            .ok_or_else(|| Error::custom("Erreur checked_add"))?;
        Ok(instant)
    }
}
