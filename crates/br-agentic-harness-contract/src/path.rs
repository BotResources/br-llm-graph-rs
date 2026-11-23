use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidFilePath {
    #[error("the path {0:?} is empty")]
    Empty(String),
    #[error("the path {0:?} is absolute")]
    Absolute(String),
    #[error("the path {0:?} leaves its folder")]
    Escapes(String),
    #[error("the path {0:?} holds a character that a file path does not")]
    Character(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FilePath(String);

impl FilePath {
    pub fn new(text: &str) -> Result<Self, InvalidFilePath> {
        if text.contains('\0') || text.contains('\\') {
            return Err(InvalidFilePath::Character(text.to_owned()));
        }
        if text.starts_with('/') {
            return Err(InvalidFilePath::Absolute(text.to_owned()));
        }
        let mut parts = Vec::new();
        for part in text.split('/') {
            match part {
                "" | "." => {}
                ".." => return Err(InvalidFilePath::Escapes(text.to_owned())),
                other => parts.push(other),
            }
        }
        if parts.is_empty() {
            return Err(InvalidFilePath::Empty(text.to_owned()));
        }
        Ok(Self(parts.join("/")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_under(&self, folder: &FilePath) -> bool {
        self.0 == folder.0
            || self
                .0
                .strip_prefix(folder.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
    }
}

impl fmt::Display for FilePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
