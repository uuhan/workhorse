use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};
pub mod v1;

pub const HEADER_SIZE: usize = std::mem::size_of::<Header>();

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Clone, Debug)]
#[repr(packed)]
pub struct Header {
    pub version: u8,
    pub size: u16,
}
