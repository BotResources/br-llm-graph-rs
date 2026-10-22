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

use crate::graph::Target;
use crate::runner::{RunnerError, RunnerTarget};
use crate::state::Kind;
use crate::value::{EndLabel, Key, NodeId, ThreadId};

pub use fault::NodeFault;

/// The one error of the crate: a variant per refused rule, carrying what
/// explains it.
///
/// A build refusal of a node reaches the caller inside `InvalidNode`; a node
/// failure inside `NodeFailed`; a failure of a called graph inside
/// `SubGraphFailed`, inside the `NodeFailed` of the calling node. An edge
/// refusal (`UndeclaredTarget`, `ToolLimitReached`, a refused `Count`) is the
/// run's error itself, and so is a refusal before the first superstep
/// (`StateSchemaMismatch`, `ConfigSchemaMismatch`, `UnknownNode`,
/// `EmptyCursor`). The source (`std::error::Error::source`) of `NodeFailed`
/// is its fault, of `Message` the br-llm-messages error, of `Model` the
/// `ClientError`, of `Request` the `BuildError`, of `SubGraphFailed` the
/// child's error, of `InvalidNode` the reason, of `Thread` the store's error,
/// of `Runner` the runner port's error.
#[derive(Debug)]
pub enum GraphError {
    /// A key, a node id or an end label that does not match
    /// `[a-z][a-z0-9_]*`.
    Identifier {
        /// The field refused: `key`, `node_id` or `end_label`.
        field: &'static str,
        /// The text refused.
        value: String,
    },
    /// A float that is NaN or infinite.
    FloatNotFinite,
    /// A text that is not a UUIDv7 written as its text, given as a thread id.
    ThreadId {
        /// The text refused.
        value: String,
    },
    /// A declared key without a value.
    MissingKey {
        /// The key.
        key: Key,
    },
    /// A key the schema does not declare.
    UnknownKey {
        /// The key.
        key: Key,
    },
    /// A key holding, or given, a value of another kind.
    KindMismatch {
        /// The key.
        key: Key,
        /// The kind the key expects (for an append, the element kind; for a
        /// list read of a key that is not a list, a list of its kind).
        expected: Kind,
        /// The type of the value found.
        found: &'static str,
    },
    /// A state or a configuration reloaded from JSON under another schema
    /// version than `br-llm-graph/1`.
    SchemaMismatch {
        /// The schema version found.
        found: String,
    },
    /// Two `Set` of one key in one batch.
    SetConflict {
        /// The key set twice.
        key: Key,
    },
    /// An `Append` to a declared key that is not a list.
    AppendNotList {
        /// The key.
        key: Key,
    },
    /// A conversation update of a declared key that is not a conversation.
    NotConversation {
        /// The key.
        key: Key,
    },
    /// A refusal of a conversation, or of its render, by br-llm-messages.
    Message(MessageError),
    /// Two nodes of a graph with one id.
    DuplicateNode {
        /// The id.
        id: NodeId,
    },
    /// A graph built without an entry.
    MissingEntry,
    /// An entry that is not a node of the graph.
    UnknownEntry {
        /// The entry.
        id: NodeId,
    },
    /// A node without an edge.
    NodeWithoutEdge {
        /// The node.
        id: NodeId,
    },
    /// An edge from a node the graph does not hold.
    EdgeFromUnknownNode {
        /// The node the edge starts from.
        id: NodeId,
    },
    /// Two edges from one node.
    DuplicateEdge {
        /// The node.
        id: NodeId,
    },
    /// A node target an edge declares that is not a node of the graph: a
    /// build refusal of the graph, checked after the nodes.
    EdgeToUnknownNode {
        /// The node whose edge it is.
        node: NodeId,
        /// The node target the graph does not hold.
        target: NodeId,
    },
    /// A target, or a cursor, naming a node the graph does not hold.
    UnknownNode {
        /// The node named.
        id: NodeId,
    },
    /// An edge that returned no target.
    EmptyEdge {
        /// The node whose edge it is.
        node: NodeId,
    },
    /// A target a declared edge returned without declaring it: the run's
    /// error itself, raised before any target runs.
    UndeclaredTarget {
        /// The node whose edge it is.
        node: NodeId,
        /// The target returned.
        target: Target,
    },
    /// A map whose list key is not a list of the item key's kind.
    MapKeyMismatch {
        /// The list key.
        list: Key,
        /// The item key.
        item: Key,
    },
    /// A node that returned an error, panicked or had its updates refused:
    /// the first failing node of the superstep in registration order.
    NodeFailed {
        /// The node.
        node: NodeId,
        /// What failed it.
        source: NodeFault,
    },
    /// A final superstep whose edges returned several distinct end labels.
    AmbiguousEnd {
        /// The labels, each once.
        labels: Vec<EndLabel>,
    },
    /// A run given a cursor with no node to run.
    EmptyCursor,
    /// A run or a session given a state not made from the graph's schema.
    StateSchemaMismatch,
    /// A run or a session given a configuration not made from the graph's
    /// schema.
    ConfigSchemaMismatch,
    /// A declared tool no tool node runs.
    ToolNotCovered {
        /// The tool.
        name: ToolName,
    },
    /// A tool run by two tool nodes, or twice by one.
    ToolCoveredTwice {
        /// The tool.
        name: ToolName,
    },
    /// A tool of a tool node the agent does not declare.
    ToolNotDeclared {
        /// The tool.
        name: ToolName,
    },
    /// An enabled key that is not a list-of-strings state key.
    EnabledKeyMismatch {
        /// The key.
        key: Key,
    },
    /// The client refused or failed the call.
    Model(ClientError),
    /// The request of a model call could not be built.
    Request(BuildError),
    /// A step without a structured block, or with one that does not read as
    /// the type asked for; an unreadable verdict after the last retry.
    Structured {
        /// Why it could not be read.
        message: String,
    },
    /// A `Limit` read from a key that is not an int configuration key.
    LimitKeyMismatch {
        /// The key.
        key: Key,
    },
    /// A `Limit` whose configuration value is below one.
    LimitNotPositive {
        /// The configuration key.
        key: Key,
        /// The value it holds.
        value: i64,
    },
    /// A `Count` read from a key that is not an int configuration key.
    CountKeyMismatch {
        /// The key.
        key: Key,
    },
    /// A `Count` whose configuration value is negative.
    CountNegative {
        /// The configuration key.
        key: Key,
        /// The value it holds.
        value: i64,
    },
    /// The two seats of a generator-critic with one author.
    SameAuthor {
        /// The author.
        author: Author,
    },
    /// A verdict override without a reader whose shape does not hold
    /// `is_valid` (boolean) and `message` (string), both required.
    VerdictShapeMismatch {
        /// What the shape lacks.
        reason: &'static str,
    },
    /// A key an agent declares that its graph already has with another
    /// meaning: another kind, or a key the graph keeps for itself.
    KeyCollision {
        /// The key.
        key: Key,
    },
    /// A round-limit flag that is not a bool state key, or that names a key
    /// no flag may name.
    FlagKeyMismatch {
        /// The key.
        key: Key,
    },
    /// The thread store failed or refused a write: its error, which the
    /// host can downcast to its own type.
    Thread {
        /// The store's error.
        source: Arc<dyn std::error::Error + Send + Sync>,
    },
    /// The thread a durable agent was given is absent from its store: the
    /// library never creates a thread it was given.
    ThreadNotFound {
        /// The thread id given.
        thread: ThreadId,
    },
    /// The `opening` of a durable agent holds no user input, or something
    /// other than user inputs.
    OpeningNotInputs,
    /// The child of a runner call failed as expected: it refused, failed,
    /// was cancelled by someone else, or did not answer in time. Its message
    /// is the reason, as the child gave it, so a `Capture` from the reason
    /// records it so.
    RunnerFailed {
        /// The runner type the call targets.
        target: RunnerTarget,
        /// Why the child gave no answer, as the runner port says.
        reason: String,
    },
    /// The runner port failed: its error, which the host can downcast to its
    /// own type.
    Runner {
        /// The port's error.
        source: RunnerError,
    },
    /// A blank text given as the target of a runner call.
    RunnerTargetBlank,
    /// A reply that asks for tools after the last round the round limit
    /// allows.
    ToolLimitReached {
        /// The model call's node.
        node: NodeId,
        /// The limit.
        max_rounds: usize,
    },
    /// A key declared twice as an input of a graph.
    DuplicateInput {
        /// The key.
        key: Key,
    },
    /// A key declared twice as an output of a graph.
    DuplicateOutput {
        /// The key.
        key: Key,
    },
    /// A declared input of a graph not given to its start state.
    MissingInput {
        /// The input.
        key: Key,
    },
    /// A key given to a start state that is not a declared input.
    NotAnInput {
        /// The key.
        key: Key,
    },
    /// An input given twice to a start state.
    InputGivenTwice {
        /// The input.
        key: Key,
    },
    /// A text that does not parse as an occurrence.
    InvalidOccurrence {
        /// The text refused.
        value: String,
    },
    /// A call mapping a key that is not a declared input of the child.
    SubGraphNotAnInput {
        /// The child's key.
        key: Key,
    },
    /// A call mapping one input of the child twice.
    SubGraphInputTwice {
        /// The child's input.
        key: Key,
    },
    /// A declared input of the child the call leaves unmapped.
    SubGraphInputUnmapped {
        /// The child's input.
        key: Key,
    },
    /// A call mapping an input of the child from a source missing or of
    /// another kind.
    SubGraphSourceMismatch {
        /// The child's input.
        key: Key,
    },
    /// A call mapping a key that is not a configuration key of the child.
    SubGraphNotAConfig {
        /// The child's key.
        key: Key,
    },
    /// A call mapping one configuration key of the child twice.
    SubGraphConfigTwice {
        /// The child's configuration key.
        key: Key,
    },
    /// A configuration key of the child the call leaves unmapped.
    SubGraphConfigUnmapped {
        /// The child's configuration key.
        key: Key,
    },
    /// A call mapping a configuration key of the child from a source
    /// missing or of another kind.
    SubGraphConfigMismatch {
        /// The child's configuration key.
        key: Key,
    },
    /// A call mapping a key that is not a declared output of the child.
    SubGraphNotAnOutput {
        /// The child's key.
        key: Key,
    },
    /// A call writing an output, or the end label, into a caller key missing
    /// or unable to take the value.
    SubGraphTargetMismatch {
        /// The caller's key.
        key: Key,
    },
    /// Two outputs of a call, end label included, that `Set` one caller key.
    SubGraphSetTwice {
        /// The caller's key.
        key: Key,
    },
    /// The called graph failed and the call propagates it.
    SubGraphFailed {
        /// The child's error.
        source: Box<GraphError>,
    },
    /// The called graph stopped before its end.
    SubGraphSuspended,
    /// A map body that returned an update other than an append to a list.
    MapBodyNotAppend {
        /// The key of the update.
        key: Key,
    },
    /// A `Set` where a map body may only append: an output, an end label
    /// target or a capture of a call used as a map body, an LLM call's
    /// output, or an item-failure capture.
    MapBodySet {
        /// The key that would be set.
        key: Key,
    },
    /// A map with an item to run whose context names no node.
    MapWithoutOccurrence,
    /// A capture update whose target cannot take its source.
    CaptureMismatch {
        /// The target key.
        key: Key,
    },
    /// Two capture updates that `Set` one caller key.
    CaptureSetTwice {
        /// The caller's key.
        key: Key,
    },
    /// A node whose check failed when the graph was built.
    InvalidNode {
        /// The node.
        node: NodeId,
        /// The reason.
        source: Box<GraphError>,
    },
}
