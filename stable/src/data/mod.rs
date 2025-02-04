use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
