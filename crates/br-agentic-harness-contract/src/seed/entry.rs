use crate::path::FilePath;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeedKind {
    File(Vec<u8>),
    Directory,
    Symlink(FilePath),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedEntry {
    pub path: FilePath,
    pub mode: u32,
    pub kind: SeedKind,
}

impl SeedEntry {
    pub fn file(path: FilePath, mode: u32, bytes: Vec<u8>) -> Self {
        Self {
            path,
            mode,
            kind: SeedKind::File(bytes),
        }
    }

    pub fn directory(path: FilePath, mode: u32) -> Self {
        Self {
            path,
            mode,
            kind: SeedKind::Directory,
        }
    }

    pub fn symlink(path: FilePath, mode: u32, target: FilePath) -> Self {
        Self {
            path,
            mode,
            kind: SeedKind::Symlink(target),
        }
    }
}
