mod entry;
mod hash;

pub use entry::{SeedEntry, SeedKind};
pub use hash::ContentHash;

use crate::path::FilePath;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidSeed {
    #[error("the seed holds the path {0} twice")]
    DuplicatePath(FilePath),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seed {
    entries: Vec<SeedEntry>,
    hash: ContentHash,
}

impl Seed {
    pub fn new(mut entries: Vec<SeedEntry>) -> Result<Self, InvalidSeed> {
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        if let Some(pair) = entries.windows(2).find(|pair| pair[0].path == pair[1].path) {
            return Err(InvalidSeed::DuplicatePath(pair[0].path.clone()));
        }
        let hash = ContentHash::of_sorted(&entries);
        Ok(Self { entries, hash })
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[SeedEntry] {
        &self.entries
    }

    pub fn content_hash(&self) -> ContentHash {
        self.hash
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for Seed {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            hash: ContentHash::of_sorted(&[]),
        }
    }
}
