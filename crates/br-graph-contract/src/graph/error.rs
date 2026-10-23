use crate::{Cause, Reason, Target};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BuildError {
    #[error("invalid version {version:?}")]
    InvalidVersion { version: String },
    #[error("invalid node name {name:?}")]
    InvalidNodeName { name: String },
    #[error("invalid kind {kind:?} on node {node:?}")]
    InvalidNodeKind { node: String, kind: String },
    #[error("invalid subkind {subkind:?} on node {node:?}")]
    InvalidNodeSubkind { node: String, subkind: String },
    #[error("invalid edge name {name:?}")]
    InvalidEdgeName { name: String },
    #[error("two nodes are named {name:?}")]
    DuplicateNodeName { name: String },
    #[error("two edges are named {name:?}")]
    DuplicateEdgeName { name: String },
    #[error("the graph has no node")]
    NoNodes,
    #[error("the graph has no entry")]
    NoEntry,
    #[error("entry {name:?} is listed twice")]
    DuplicateEntry { name: String },
    #[error("entry {name:?} is not a node")]
    EntryNotANode { name: String },
    #[error("the source {source_node:?} of edge {edge:?} is not a node")]
    EdgeSourceNotANode { edge: String, source_node: String },
    #[error("node {node:?} has no edge")]
    NodeWithoutEdge { node: String },
    #[error("node {node:?} has more than one edge")]
    NodeWithSeveralEdges { node: String },
    #[error("edge {edge:?} declares no target")]
    EmptyTargets { edge: String },
    #[error("edge {edge:?} declares target {target} twice")]
    DuplicateTarget { edge: String, target: Target },
    #[error("edge {edge:?} declares target {target}, which is neither a node nor END")]
    UnknownTarget { edge: String, target: Target },
    #[error("node {node:?} is not reachable from any entry")]
    UnreachableNode { node: String },
    #[error("END cannot be reached from node {node:?}")]
    EndUnreachable { node: String },
}

#[derive(Debug, thiserror::Error)]
pub enum EdgeError {
    #[error("the graph holds no node named {node:?}")]
    UnknownNode { node: String },
    #[error("the edge of node {node:?} gave target {target}, which it did not declare")]
    UndeclaredTarget { node: String, target: Target },
    #[error("the edge of node {node:?} gave no target")]
    EmptyTargets { node: String },
    #[error("the edge of node {node:?} mixed END with node names")]
    EndMixedWithNodes { node: String },
    #[error("the edge failed")]
    Failed(#[source] Cause),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StructureError {
    #[error("malformed structure: {0}")]
    Malformed(Reason),
    #[error("structure breaks a build rule: {0}")]
    Rule(#[from] BuildError),
}

impl StructureError {
    pub fn reason(&self) -> Reason {
        match self {
            Self::Malformed(reason) => reason.clone(),
            Self::Rule(error) => Reason::new(error.to_string()),
        }
    }
}
