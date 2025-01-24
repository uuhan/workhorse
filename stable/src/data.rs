use serde::{Deserialize, Serialize};
use std::path::PathBuf;
pub use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

pub const HEADER_SIZE: usize = std::mem::size_of::<Header>();

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Clone, Debug)]
#[repr(C)]
pub struct Header {
    pub size: u16,
}

#[derive(Serialize, Deserialize)]
pub struct GetFile {
    pub path: PathBuf,
    pub kind: GetKind,
    pub size: Option<u64>,
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_header_size() {
        assert_eq!(HEADER_SIZE, 1);
    }

    #[test]
    fn test_get_file_size() {
        let file = GetFile {
            path: PathBuf::from("/foo/bar/"),
            kind: GetKind::File,
            size: None,
        };

        assert_eq!(bincode::serialize(&file).unwrap().len(), 22);
    }
}
