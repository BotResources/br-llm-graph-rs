use br_llm_messages::MessageError;
use br_llm_messages::ToolName;

use crate::state::Kind;
use crate::value::{EndLabel, Key, NodeId};

#[derive(Debug)]
pub enum NodeFault {
    Returned(String),
    Refused(Box<GraphError>),
    Panic(String),
}

impl std::fmt::Display for NodeFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeFault::Returned(message) => write!(f, "returned an error: {message}"),
            NodeFault::Refused(error) => write!(f, "its updates were refused: {error}"),
            NodeFault::Panic(message) => write!(f, "panicked: {message}"),
        }
    }
}

#[derive(Debug)]
pub enum GraphError {
    Identifier {
        field: &'static str,
        value: String,
    },
    FloatNotFinite,
    MissingKey {
        key: Key,
    },
    UnknownKey {
        key: Key,
    },
    KindMismatch {
        key: Key,
        expected: Kind,
        found: &'static str,
    },
    SchemaMismatch {
        found: String,
    },
    SetConflict {
        key: Key,
    },
    AppendNotList {
        key: Key,
    },
    NotConversation {
        key: Key,
    },
    Message(MessageError),
    DuplicateNode {
        id: NodeId,
    },
    MissingEntry,
    UnknownEntry {
        id: NodeId,
    },
    NodeWithoutEdge {
        id: NodeId,
    },
    EdgeFromUnknownNode {
        id: NodeId,
    },
    UnknownNode {
        id: NodeId,
    },
    EmptyEdge {
        node: NodeId,
    },
    MapKeyMismatch {
        node: NodeId,
    },
    NodeFailed {
        node: NodeId,
        source: NodeFault,
    },
    AmbiguousEnd {
        labels: Vec<EndLabel>,
    },
    BadInboxInput {
        key: Key,
    },
    ToolNotCovered {
        name: ToolName,
    },
    ToolCoveredTwice {
        name: ToolName,
    },
    Model {
        message: String,
    },
    Structured {
        message: String,
    },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::Identifier { field, value } => {
                write!(
                    f,
                    "{field} {value:?} is not a valid identifier [a-z][a-z0-9_]*"
                )
            }
            GraphError::FloatNotFinite => f.write_str("a float value must be finite"),
            GraphError::MissingKey { key } => write!(f, "key {key} is declared but missing"),
            GraphError::UnknownKey { key } => write!(f, "key {key} is not declared in the schema"),
            GraphError::KindMismatch {
                key,
                expected,
                found,
            } => write!(
                f,
                "key {key} expects kind {expected} but found a {found} value"
            ),
            GraphError::SchemaMismatch { found } => write!(f, "unexpected schema {found:?}"),
            GraphError::SetConflict { key } => {
                write!(f, "key {key} is set twice in one batch")
            }
            GraphError::AppendNotList { key } => {
                write!(f, "append requires key {key} to be a list")
            }
            GraphError::NotConversation { key } => {
                write!(f, "key {key} is not a conversation")
            }
            GraphError::Message(error) => write!(f, "message error: {error}"),
            GraphError::DuplicateNode { id } => write!(f, "node id {id} is registered twice"),
            GraphError::MissingEntry => f.write_str("the graph has no entry node"),
            GraphError::UnknownEntry { id } => write!(f, "entry {id} is not a registered node"),
            GraphError::NodeWithoutEdge { id } => write!(f, "node {id} has no edge"),
            GraphError::EdgeFromUnknownNode { id } => {
                write!(f, "an edge starts from unknown node {id}")
            }
            GraphError::UnknownNode { id } => write!(f, "node {id} is not in the graph"),
            GraphError::EmptyEdge { node } => {
                write!(f, "the edge of node {node} returned no target")
            }
            GraphError::MapKeyMismatch { node } => {
                write!(
                    f,
                    "the map node {node} has inconsistent list/item/output/results keys"
                )
            }
            GraphError::NodeFailed { node, source } => write!(f, "node {node} failed: {source}"),
            GraphError::AmbiguousEnd { labels } => {
                let labels: Vec<&str> = labels.iter().map(EndLabel::as_str).collect();
                write!(
                    f,
                    "the run ended with several labels [{}]",
                    labels.join(", ")
                )
            }
            GraphError::BadInboxInput { key } => {
                write!(f, "an inbox input targets the invalid key {key}")
            }
            GraphError::ToolNotCovered { name } => {
                write!(
                    f,
                    "tool {name} is declared on the llm node but no tool node runs it"
                )
            }
            GraphError::ToolCoveredTwice { name } => {
                write!(f, "tool {name} is run by more than one tool node")
            }
            GraphError::Model { message } => write!(f, "model call failed: {message}"),
            GraphError::Structured { message } => {
                write!(f, "structured output could not be read: {message}")
            }
        }
    }
}

impl std::error::Error for GraphError {}

impl From<MessageError> for GraphError {
    fn from(error: MessageError) -> Self {
        GraphError::Message(error)
    }
}
