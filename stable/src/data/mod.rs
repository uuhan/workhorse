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
