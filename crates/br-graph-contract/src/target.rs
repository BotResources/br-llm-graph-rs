use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const END_TARGET: &str = "END";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Target {
    Node(String),
    End,
}

impl Target {
    pub const END: Target = Target::End;

    pub fn node(name: impl Into<String>) -> Self {
        Self::Node(name.into())
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Node(name) => name,
            Self::End => END_TARGET,
        }
    }

    pub fn is_end(&self) -> bool {
        matches!(self, Self::End)
    }
}

impl From<&str> for Target {
    fn from(text: &str) -> Self {
        if text == END_TARGET {
            Self::End
        } else {
            Self::Node(text.to_owned())
        }
    }
}

impl From<String> for Target {
    fn from(text: String) -> Self {
        if text == END_TARGET {
            Self::End
        } else {
            Self::Node(text)
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Target {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Target {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::from)
    }
}
