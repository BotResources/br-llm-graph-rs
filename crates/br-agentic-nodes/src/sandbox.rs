use std::collections::BTreeMap;
use std::fmt;

use br_agentic_contract::WorkspaceFile;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SandboxError {
    #[error("the path {0:?} is empty")]
    Empty(String),
    #[error("the path {0:?} is absolute")]
    Absolute(String),
    #[error("the path {0:?} leaves the file system")]
    Escapes(String),
    #[error("the path {0:?} holds a character that a path of the file system does not")]
    Character(String),
    #[error("the path {0:?} appears twice")]
    Duplicate(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SandboxPath(String);

impl SandboxPath {
    pub fn new(text: &str) -> Result<Self, SandboxError> {
        if text.contains('\0') || text.contains('\\') {
            return Err(SandboxError::Character(text.to_owned()));
        }
        if text.starts_with('/') {
            return Err(SandboxError::Absolute(text.to_owned()));
        }
        let mut parts = Vec::new();
        for part in text.split('/') {
            match part {
                "" | "." => {}
                ".." => return Err(SandboxError::Escapes(text.to_owned())),
                other => parts.push(other),
            }
        }
        if parts.is_empty() {
            return Err(SandboxError::Empty(text.to_owned()));
        }
        Ok(Self(parts.join("/")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SandboxPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileSystem {
    files: BTreeMap<SandboxPath, Vec<u8>>,
}

impl FileSystem {
    pub fn from_files(files: Vec<WorkspaceFile>) -> Result<Self, SandboxError> {
        let mut system = Self::default();
        for file in files {
            let path = SandboxPath::new(&file.path)?;
            if system.files.insert(path.clone(), file.content).is_some() {
                return Err(SandboxError::Duplicate(path.to_string()));
            }
        }
        Ok(system)
    }

    pub fn read(&self, path: &str) -> Result<Option<&[u8]>, SandboxError> {
        let path = SandboxPath::new(path)?;
        Ok(self.files.get(&path).map(Vec::as_slice))
    }

    pub fn write(&mut self, path: &str, content: Vec<u8>) -> Result<(), SandboxError> {
        self.files.insert(SandboxPath::new(path)?, content);
        Ok(())
    }

    pub fn remove(&mut self, path: &str) -> Result<bool, SandboxError> {
        Ok(self.files.remove(&SandboxPath::new(path)?).is_some())
    }

    pub fn paths(&self) -> impl Iterator<Item = &SandboxPath> {
        self.files.keys()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn into_files(self) -> Vec<WorkspaceFile> {
        self.files
            .into_iter()
            .map(|(path, content)| WorkspaceFile {
                path: path.0,
                content,
            })
            .collect()
    }
}
