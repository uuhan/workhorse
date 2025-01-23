use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

pub const HEADER_SIZE: usize = std::mem::size_of::<Header>();

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable)]
#[repr(C)]
pub struct Header {
    pub size: usize,
}

#[derive(Serialize, Deserialize)]
pub struct GetFile {
    pub path: PathBuf,
    pub kind: GetKind,
    pub size: u64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GetKind {
    File,
    Directory,
}

impl GetKind {
    pub fn is_file(&self) -> bool {
        matches!(self, GetKind::File)
    }

    pub fn is_dir(&self) -> bool {
        matches!(self, GetKind::Directory)
    }
}
