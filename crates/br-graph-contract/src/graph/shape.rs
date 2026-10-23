use serde::Deserialize;

use super::structure::{StructureEdge, StructureNode};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shape {
    pub(super) version: String,
    pub(super) entries: Vec<String>,
    pub(super) nodes: Vec<StructureNode>,
    pub(super) edges: Vec<StructureEdge>,
}
