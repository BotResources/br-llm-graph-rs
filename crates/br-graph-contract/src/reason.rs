use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Reason(String);

impl Reason {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Reason {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for Reason {
    fn from(text: String) -> Self {
        Self(text)
    }
}

impl From<&str> for Reason {
    fn from(text: &str) -> Self {
        Self(text.to_owned())
    }
}
