//! The identifiers of the crate: `Key`, `NodeId` and `EndLabel`, and the
//! `ThreadId` of an agent's thread.

mod thread_id;

pub use thread_id::ThreadId;

pub(crate) fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        Some(_) | None => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

macro_rules! identifier_newtype {
    ($name:ident, $field:literal, $doc:literal) => {
        #[doc = $doc]
        ///
        /// It matches `[a-z][a-z0-9_]*` (ASCII); anything else, the empty
        /// string included, is refused (`Identifier`), from JSON too.
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
        #[serde(into = "String")]
        pub struct $name(String);

        impl $name {
            /// `value`, refused when it is not an identifier (`Identifier`).
            pub fn new(value: impl Into<String>) -> Result<Self, $crate::error::GraphError> {
                let value = value.into();
                if $crate::value::is_identifier(&value) {
                    Ok(Self(value))
                } else {
                    Err($crate::error::GraphError::Identifier {
                        field: $field,
                        value,
                    })
                }
            }

            /// Its text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl std::convert::TryFrom<String> for $name {
            type Error = $crate::error::GraphError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl std::convert::From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let raw = String::deserialize(deserializer)?;
                Self::new(raw).map_err(serde::de::Error::custom)
            }
        }
    };
}

identifier_newtype!(Key, "key", "The name of a state or configuration key.");
identifier_newtype!(NodeId, "node_id", "The id of a node of a graph.");
identifier_newtype!(EndLabel, "end_label", "The label a run ends with.");

#[cfg(test)]
#[path = "value_tests.rs"]
mod tests;
