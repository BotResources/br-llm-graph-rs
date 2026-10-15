use crate::error::GraphError;

/// The id of a thread: a UUIDv7, written as its text.
///
/// The text is the hyphenated form of RFC 9562,
/// `xxxxxxxx-xxxx-7xxx-Vxxx-xxxxxxxxxxxx`: 32 hexadecimal digits in groups of
/// 8, 4, 4, 4 and 12, the version digit `7`, the variant digit `8`, `9`, `a`
/// or `b`. Hexadecimal digits are read in either case, as RFC 9562 reads
/// them, and the text is kept as written: the id names the thread in the
/// host's store under the very text the host gave.
/// Any other text, the empty string included, is refused (`ThreadId`); so is
/// JSON, as a deserialization error carrying the refusal's message.
///
/// The library checks the form only: generating ids is the id source's.
///
/// ```
/// use br_llm_graph::{GraphError, ThreadId};
///
/// let id = ThreadId::new("01a1402a-1fc1-7cff-9991-d1720933c79a").unwrap();
/// assert_eq!(id.as_str(), "01a1402a-1fc1-7cff-9991-d1720933c79a");
/// assert!(matches!(
///     ThreadId::new("thread-1"),
///     Err(GraphError::ThreadId { .. })
/// ));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(into = "String")]
pub struct ThreadId(String);

impl ThreadId {
    /// The thread id written `value`, refused (`ThreadId`) unless it is the
    /// text of a UUIDv7.
    pub fn new(value: impl Into<String>) -> Result<Self, GraphError> {
        let value = value.into();
        if is_uuid_v7(&value) {
            Ok(Self(value))
        } else {
            Err(GraphError::ThreadId { value })
        }
    }

    /// The text of the id, as written.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Byte offsets of the hyphens, the version digit and the variant digit in
/// the hyphenated text of a UUID.
const HYPHENS: [usize; 4] = [8, 13, 18, 23];
const VERSION: usize = 14;
const VARIANT: usize = 19;

fn is_uuid_v7(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| {
            if HYPHENS.contains(&index) {
                *byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
        && bytes.get(VERSION) == Some(&b'7')
        && matches!(
            bytes.get(VARIANT),
            Some(b'8' | b'9' | b'a' | b'b' | b'A' | b'B')
        )
}

impl std::fmt::Display for ThreadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ThreadId {
    type Error = GraphError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ThreadId> for String {
    fn from(value: ThreadId) -> Self {
        value.0
    }
}

impl<'de> serde::Deserialize<'de> for ThreadId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[path = "thread_id_tests.rs"]
mod tests;
