use std::fmt;

use sha2::{Digest, Sha256};

use super::entry::{SeedEntry, SeedKind};

const DOMAIN: &[u8] = b"br-agentic-harness-contract/seed/1\n";
const FILE: u8 = 1;
const DIRECTORY: u8 = 2;
const SYMLINK: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    pub(super) fn of_sorted(entries: &[SeedEntry]) -> Self {
        let mut digest = Sha256::new();
        digest.update(DOMAIN);
        digest.update(length(entries.len()));
        for entry in entries {
            digest.update(sized(entry.path.as_str().as_bytes()));
            digest.update(entry.mode.to_be_bytes());
            match &entry.kind {
                SeedKind::File(bytes) => {
                    digest.update([FILE]);
                    digest.update(sized(bytes));
                }
                SeedKind::Directory => digest.update([DIRECTORY]),
                SeedKind::Symlink(target) => {
                    digest.update([SYMLINK]);
                    digest.update(sized(target.as_str().as_bytes()));
                }
            }
        }
        Self(digest.finalize().into())
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

fn length(count: usize) -> [u8; 8] {
    u64::try_from(count).unwrap_or(u64::MAX).to_be_bytes()
}

fn sized(bytes: &[u8]) -> Vec<u8> {
    let mut framed = length(bytes.len()).to_vec();
    framed.extend_from_slice(bytes);
    framed
}
