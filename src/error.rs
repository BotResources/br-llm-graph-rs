use br_llm_messages::MessageError;
use br_llm_messages::ToolName;

use crate::graph::NodeError;
use crate::state::Kind;
use crate::value::{EndLabel, Key, NodeId};

#[derive(Debug)]
pub enum NodeFault {
    Returned(NodeError),
    Refused(Box<GraphError>),
    Panic(String),
}

impl std::fmt::Display for NodeFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeFault::Returned(error) => write!(f, "returned an error: {error}"),
            NodeFault::Refused(error) => write!(f, "its updates were refused: {error}"),
            NodeFault::Panic(message) => write!(f, "panicked: {message}"),
        }
    }
}

impl std::error::Error for NodeFault {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NodeFault::Returned(error) => Some(error.as_ref()),
            NodeFault::Refused(error) => Some(error.as_ref()),
            NodeFault::Panic(_) => None,
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
    DuplicateEdge {
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
    ToolNotCovered {
        name: ToolName,
    },
    ToolCoveredTwice {
        name: ToolName,
    },
    ToolNotDeclared {
        name: ToolName,
    },
    PendingToolUnsatisfiable {
        name: ToolName,
    },
    Model {
        message: String,
    },
    Structured {
        message: String,
    },
    LimitKeyMismatch {
        key: Key,
    },
    LimitNotPositive {
        key: Key,
        value: i64,
    },
    FlagKeyMismatch {
        key: Key,
    },
    ToolLimitReached {
        node: NodeId,
        max_rounds: usize,
    },
    DuplicateInput {
        key: Key,
    },
    DuplicateOutput {
        key: Key,
    },
    MissingInput {
        key: Key,
    },
    NotAnInput {
        key: Key,
    },
    InputGivenTwice {
        key: Key,
    },
    InvalidOccurrence {
        value: String,
    },
    SubGraphNotAnInput {
        node: NodeId,
        key: Key,
    },
    SubGraphInputTwice {
        node: NodeId,
        key: Key,
    },
    SubGraphInputUnmapped {
        node: NodeId,
        key: Key,
    },
    SubGraphSourceMismatch {
        node: NodeId,
        key: Key,
    },
    SubGraphNotAConfig {
        node: NodeId,
        key: Key,
    },
    SubGraphConfigTwice {
        node: NodeId,
        key: Key,
    },
    SubGraphConfigUnmapped {
        node: NodeId,
        key: Key,
    },
    SubGraphConfigMismatch {
        node: NodeId,
        key: Key,
    },
    SubGraphNotAnOutput {
        node: NodeId,
        key: Key,
    },
    SubGraphTargetMismatch {
        node: NodeId,
        key: Key,
    },
    SubGraphFailed {
        source: Box<GraphError>,
    },
    SubGraphSuspended,
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
            GraphError::DuplicateEdge { id } => {
                write!(f, "node {id} has more than one edge")
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
            GraphError::ToolNotCovered { name } => {
                write!(
                    f,
                    "tool {name} is declared on the llm node but no tool node runs it"
                )
            }
            GraphError::ToolCoveredTwice { name } => {
                write!(f, "tool {name} is run by more than one tool node")
            }
            GraphError::ToolNotDeclared { name } => {
                write!(
                    f,
                    "tool node runs tool {name} the llm node does not declare"
                )
            }
            GraphError::PendingToolUnsatisfiable { name } => {
                write!(f, "pending tool call {name} names a tool no tool node runs")
            }
            GraphError::Model { message } => write!(f, "model call failed: {message}"),
            GraphError::Structured { message } => {
                write!(f, "structured output could not be read: {message}")
            }
            GraphError::LimitKeyMismatch { key } => {
                write!(f, "limit key {key} is not an int configuration key")
            }
            GraphError::LimitNotPositive { key, value } => {
                write!(f, "limit key {key} holds {value}, not a positive integer")
            }
            GraphError::FlagKeyMismatch { key } => {
                write!(f, "flag key {key} is not a bool state key")
            }
            GraphError::ToolLimitReached { node, max_rounds } => {
                write!(
                    f,
                    "node {node} reached its limit of {max_rounds} tool rounds"
                )
            }
            GraphError::DuplicateInput { key } => {
                write!(f, "key {key} is declared twice as an input of the graph")
            }
            GraphError::DuplicateOutput { key } => {
                write!(f, "key {key} is declared twice as an output of the graph")
            }
            GraphError::MissingInput { key } => {
                write!(f, "input {key} of the graph is not given")
            }
            GraphError::NotAnInput { key } => {
                write!(f, "key {key} is not a declared input of the graph")
            }
            GraphError::InputGivenTwice { key } => write!(f, "input {key} is given twice"),
            GraphError::InvalidOccurrence { value } => {
                write!(f, "{value:?} is not a valid occurrence key")
            }
            GraphError::SubGraphNotAnInput { node, key } => {
                write!(
                    f,
                    "node {node} maps {key}, which is not a declared input of the called graph"
                )
            }
            GraphError::SubGraphInputTwice { node, key } => {
                write!(f, "node {node} maps input {key} of the called graph twice")
            }
            GraphError::SubGraphInputUnmapped { node, key } => {
                write!(
                    f,
                    "node {node} does not map input {key} of the called graph"
                )
            }
            GraphError::SubGraphSourceMismatch { node, key } => {
                write!(
                    f,
                    "node {node} maps input {key} from a source that is missing or of another kind"
                )
            }
            GraphError::SubGraphNotAConfig { node, key } => {
                write!(
                    f,
                    "node {node} maps {key}, which is not a configuration key of the called graph"
                )
            }
            GraphError::SubGraphConfigTwice { node, key } => {
                write!(
                    f,
                    "node {node} maps configuration key {key} of the called graph twice"
                )
            }
            GraphError::SubGraphConfigUnmapped { node, key } => {
                write!(
                    f,
                    "node {node} does not map configuration key {key} of the called graph"
                )
            }
            GraphError::SubGraphConfigMismatch { node, key } => {
                write!(
                    f,
                    "node {node} maps configuration key {key} from a source that is missing or of another kind"
                )
            }
            GraphError::SubGraphNotAnOutput { node, key } => {
                write!(
                    f,
                    "node {node} maps {key}, which is not a declared output of the called graph"
                )
            }
            GraphError::SubGraphTargetMismatch { node, key } => {
                write!(
                    f,
                    "node {node} writes into {key}, which is missing or cannot take the value"
                )
            }
            GraphError::SubGraphFailed { source } => write!(f, "the called graph failed: {source}"),
            GraphError::SubGraphSuspended => {
                f.write_str("the called graph stopped before its end (paused or cancelled)")
            }
        }
    }
}

impl std::error::Error for GraphError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GraphError::NodeFailed { source, .. } => Some(source),
            GraphError::Message(error) => Some(error),
            GraphError::SubGraphFailed { source } => Some(source.as_ref()),
            GraphError::Identifier { .. }
            | GraphError::FloatNotFinite
            | GraphError::MissingKey { .. }
            | GraphError::UnknownKey { .. }
            | GraphError::KindMismatch { .. }
            | GraphError::SchemaMismatch { .. }
            | GraphError::SetConflict { .. }
            | GraphError::AppendNotList { .. }
            | GraphError::NotConversation { .. }
            | GraphError::DuplicateNode { .. }
            | GraphError::MissingEntry
            | GraphError::UnknownEntry { .. }
            | GraphError::NodeWithoutEdge { .. }
            | GraphError::EdgeFromUnknownNode { .. }
            | GraphError::DuplicateEdge { .. }
            | GraphError::UnknownNode { .. }
            | GraphError::EmptyEdge { .. }
            | GraphError::MapKeyMismatch { .. }
            | GraphError::AmbiguousEnd { .. }
            | GraphError::ToolNotCovered { .. }
            | GraphError::ToolCoveredTwice { .. }
            | GraphError::ToolNotDeclared { .. }
            | GraphError::PendingToolUnsatisfiable { .. }
            | GraphError::Model { .. }
            | GraphError::Structured { .. }
            | GraphError::LimitKeyMismatch { .. }
            | GraphError::LimitNotPositive { .. }
            | GraphError::FlagKeyMismatch { .. }
            | GraphError::ToolLimitReached { .. }
            | GraphError::DuplicateInput { .. }
            | GraphError::DuplicateOutput { .. }
            | GraphError::MissingInput { .. }
            | GraphError::NotAnInput { .. }
            | GraphError::InputGivenTwice { .. }
            | GraphError::InvalidOccurrence { .. }
            | GraphError::SubGraphNotAnInput { .. }
            | GraphError::SubGraphInputTwice { .. }
            | GraphError::SubGraphInputUnmapped { .. }
            | GraphError::SubGraphSourceMismatch { .. }
            | GraphError::SubGraphNotAConfig { .. }
            | GraphError::SubGraphConfigTwice { .. }
            | GraphError::SubGraphConfigUnmapped { .. }
            | GraphError::SubGraphConfigMismatch { .. }
            | GraphError::SubGraphNotAnOutput { .. }
            | GraphError::SubGraphTargetMismatch { .. }
            | GraphError::SubGraphSuspended => None,
        }
    }
}

impl From<MessageError> for GraphError {
    fn from(error: MessageError) -> Self {
        GraphError::Message(error)
    }
}
