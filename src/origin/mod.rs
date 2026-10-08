//! Where an event or a recorded write comes from: the run, and the occurrence
//! of a node inside it.

use crate::error::GraphError;
use crate::value::NodeId;

/// One step of an occurrence path: a node, and the item index when the node
/// repeats a body over a list.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Segment {
    pub node: NodeId,
    pub index: Option<usize>,
}

impl Segment {
    pub fn node(node: NodeId) -> Self {
        Self { node, index: None }
    }

    pub fn item(node: NodeId, index: usize) -> Self {
        Self {
            node,
            index: Some(index),
        }
    }
}

impl std::fmt::Display for Segment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.index {
            Some(index) => write!(f, "{}[{index}]", self.node),
            None => write!(f, "{}", self.node),
        }
    }
}

/// The path of a node occurrence through nested runs, empty at top level.
/// Written `a/b[3]/c`; serialized as that text.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default, serde::Serialize)]
#[serde(into = "String")]
pub struct OccurrenceKey(Vec<Segment>);

impl OccurrenceKey {
    pub fn root() -> Self {
        Self(Vec::new())
    }

    pub fn new(segments: Vec<Segment>) -> Self {
        Self(segments)
    }

    pub fn segments(&self) -> &[Segment] {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    pub fn child(&self, segment: Segment) -> Self {
        let mut segments = self.0.clone();
        segments.push(segment);
        Self(segments)
    }

    pub fn starts_with(&self, prefix: &OccurrenceKey) -> bool {
        self.0.starts_with(&prefix.0)
    }

    pub fn parse(text: &str) -> Result<Self, GraphError> {
        if text.is_empty() {
            return Ok(Self::root());
        }
        let invalid = || GraphError::InvalidOccurrence {
            value: text.to_owned(),
        };
        let mut segments = Vec::new();
        for part in text.split('/') {
            let segment = match part.strip_suffix(']') {
                Some(head) => {
                    let (node, digits) = head.split_once('[').ok_or_else(invalid)?;
                    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                        return Err(invalid());
                    }
                    let index = digits.parse::<usize>().map_err(|_| invalid())?;
                    Segment::item(NodeId::new(node).map_err(|_| invalid())?, index)
                }
                None => Segment::node(NodeId::new(part).map_err(|_| invalid())?),
            };
            segments.push(segment);
        }
        Ok(Self(segments))
    }
}

impl std::fmt::Display for OccurrenceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (position, segment) in self.0.iter().enumerate() {
            if position > 0 {
                f.write_str("/")?;
            }
            write!(f, "{segment}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for OccurrenceKey {
    type Err = GraphError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl From<OccurrenceKey> for String {
    fn from(key: OccurrenceKey) -> Self {
        key.to_string()
    }
}

impl<'de> serde::Deserialize<'de> for OccurrenceKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// An opaque run identifier supplied by the host. The library only carries
/// it; nested runs share the run identifier of their caller.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
pub struct RunId(String);

impl RunId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The run and the occurrence an observer event comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub struct Origin {
    pub run: RunId,
    pub occurrence: OccurrenceKey,
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.run, self.occurrence)
    }
}

#[cfg(test)]
#[path = "origin_tests.rs"]
mod tests;
