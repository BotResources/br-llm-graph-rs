use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};

use super::check::validate;
use super::shape::Shape;
use super::{BuildError, StructureError};
use crate::{Reason, Target};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureNode {
    pub name: String,
    pub kind: String,
    pub subkind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StructureEdge {
    pub name: String,
    pub source: String,
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Structure {
    version: String,
    entries: Vec<String>,
    nodes: Vec<StructureNode>,
    edges: Vec<StructureEdge>,
}

impl Structure {
    pub fn from_json_slice(json: &[u8]) -> Result<Self, StructureError> {
        let shape: Shape = serde_json::from_slice(json)
            .map_err(|error| StructureError::Malformed(Reason::new(error.to_string())))?;
        Ok(Self::from_shape(shape)?)
    }

    pub fn from_json_str(json: &str) -> Result<Self, StructureError> {
        Self::from_json_slice(json.as_bytes())
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    pub fn nodes(&self) -> &[StructureNode] {
        &self.nodes
    }

    pub fn edges(&self) -> &[StructureEdge] {
        &self.edges
    }
}

impl Structure {
    pub(super) fn from_shape(mut shape: Shape) -> Result<Self, BuildError> {
        shape.entries.sort();
        shape.nodes.sort_by(|a, b| a.name.cmp(&b.name));
        shape.edges.sort_by(|a, b| a.name.cmp(&b.name));
        validate(&shape)?;
        Ok(Self {
            version: shape.version,
            entries: shape.entries,
            nodes: shape.nodes,
            edges: shape.edges,
        })
    }
}

impl<'de> Deserialize<'de> for Structure {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let shape = Shape::deserialize(deserializer)?;
        Self::from_shape(shape).map_err(D::Error::custom)
    }
}
