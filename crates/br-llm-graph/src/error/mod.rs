//! The one error type of the crate, `GraphError`, and the fault of a failed
//! node, `NodeFault`.

mod convert;
mod display;
mod fault;
mod source;

use std::sync::Arc;

use br_llm_clients_core::{BuildError, ClientError};
use br_llm_messages::MessageError;
use br_llm_messages::{Author, ToolName};

use crate::state::Kind;
use crate::value::{EndLabel, Key, NodeId, ThreadId};

pub use fault::NodeFault;

#[derive(Debug)]
pub enum GraphError {
    Identifier {
        field: &'static str,
        value: String,
    },
    FloatNotFinite,
    /// A text that is not a UUIDv7 written as its text, given as a thread id.
    ThreadId {
        value: String,
    },
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
        list: Key,
        item: Key,
    },
    NodeFailed {
        node: NodeId,
        source: NodeFault,
    },
    AmbiguousEnd {
        labels: Vec<EndLabel>,
    },
    EmptyCursor,
    StateSchemaMismatch,
    ConfigSchemaMismatch,
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
    EnabledKeyMismatch {
        key: Key,
    },
    /// The client refused or failed the call.
    Model(ClientError),
    /// The request of a model call could not be built.
    Request(BuildError),
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
    CountKeyMismatch {
        key: Key,
    },
    CountNegative {
        key: Key,
        value: i64,
    },
    SameAuthor {
        author: Author,
    },
    VerdictShapeMismatch {
        reason: &'static str,
    },
    KeyCollision {
        key: Key,
    },
    FlagKeyMismatch {
        key: Key,
    },
    /// The thread store failed or refused a write: its error, which the
    /// host can downcast to its own type.
    Thread {
        source: Arc<dyn std::error::Error + Send + Sync>,
    },
    /// The thread a durable agent was given is absent from its store: the
    /// library never creates a thread it was given.
    ThreadNotFound {
        thread: ThreadId,
    },
    /// The `opening` of a durable agent holds no user input, or something
    /// other than user inputs.
    OpeningNotInputs,
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
        key: Key,
    },
    SubGraphInputTwice {
        key: Key,
    },
    SubGraphInputUnmapped {
        key: Key,
    },
    SubGraphSourceMismatch {
        key: Key,
    },
    SubGraphNotAConfig {
        key: Key,
    },
    SubGraphConfigTwice {
        key: Key,
    },
    SubGraphConfigUnmapped {
        key: Key,
    },
    SubGraphConfigMismatch {
        key: Key,
    },
    SubGraphNotAnOutput {
        key: Key,
    },
    SubGraphTargetMismatch {
        key: Key,
    },
    SubGraphSetTwice {
        key: Key,
    },
    SubGraphFailed {
        source: Box<GraphError>,
    },
    SubGraphSuspended,
    MapBodyNotAppend {
        key: Key,
    },
    MapBodySet {
        key: Key,
    },
    MapWithoutOccurrence,
    CaptureMismatch {
        key: Key,
    },
    CaptureSetTwice {
        key: Key,
    },
    InvalidNode {
        node: NodeId,
        source: Box<GraphError>,
    },
}
