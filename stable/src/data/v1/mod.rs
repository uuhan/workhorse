use super::*;

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
        assert_eq!(HEADER_SIZE, 3);
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
