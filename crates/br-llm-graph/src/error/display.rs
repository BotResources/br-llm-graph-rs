use crate::error::GraphError;
use crate::value::EndLabel;

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
            GraphError::ThreadId { value } => {
                write!(
                    f,
                    "{value:?} is not a thread id: a UUIDv7 written as its text"
                )
            }
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
            GraphError::MapKeyMismatch { list, item } => write!(
                f,
                "map list {list} must be a list whose element kind is the kind of item {item}"
            ),
            GraphError::NodeFailed { node, source } => write!(f, "node {node} failed: {source}"),
            GraphError::AmbiguousEnd { labels } => {
                let labels: Vec<&str> = labels.iter().map(EndLabel::as_str).collect();
                write!(
                    f,
                    "the run ended with several labels [{}]",
                    labels.join(", ")
                )
            }
            GraphError::EmptyCursor => {
                f.write_str("the run was given an empty cursor: no node to run")
            }
            GraphError::StateSchemaMismatch => {
                f.write_str("the state was not made from the schema of the graph")
            }
            GraphError::ConfigSchemaMismatch => {
                f.write_str("the configuration was not made from the schema of the graph")
            }
            GraphError::ToolNotCovered { name } => {
                write!(
                    f,
                    "tool {name} is declared on the llm node but no tool node runs it"
                )
            }
            GraphError::ToolCoveredTwice { name } => {
                write!(
                    f,
                    "tool {name} is run more than once by the tool nodes (twice in one node or in two nodes)"
                )
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
            GraphError::EnabledKeyMismatch { key } => {
                write!(f, "enabled key {key} is not a list of strings state key")
            }
            GraphError::SourceKeyMismatch { key } => write!(
                f,
                "source key {key} is not a declared key holding a string or a list of strings"
            ),
            GraphError::Model(error) => write!(f, "model call failed: {error}"),
            GraphError::Request(error) => {
                write!(f, "the request of a model call could not be built: {error}")
            }
            GraphError::Structured { message } => {
                write!(f, "structured output could not be read: {message}")
            }
            GraphError::LimitKeyMismatch { key } => {
                write!(f, "limit key {key} is not an int configuration key")
            }
            GraphError::LimitNotPositive { key, value } => {
                write!(f, "limit key {key} holds {value}, not a positive integer")
            }
            GraphError::CountKeyMismatch { key } => {
                write!(f, "count key {key} is not an int configuration key")
            }
            GraphError::CountNegative { key, value } => {
                write!(
                    f,
                    "count key {key} holds {value}, not a count of zero or more"
                )
            }
            GraphError::SameAuthor { author } => write!(
                f,
                "the generator and the critic are both written by {author}; each seat needs its own author"
            ),
            GraphError::VerdictShapeMismatch { reason } => write!(
                f,
                "the critic's verdict shape cannot be read without a reader: {reason}"
            ),
            GraphError::KeyCollision { key } => {
                write!(f, "key {key} is already a key of the graph")
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
            GraphError::SubGraphNotAnInput { key } => write!(
                f,
                "the call maps {key}, which is not a declared input of the called graph"
            ),
            GraphError::SubGraphInputTwice { key } => {
                write!(f, "the call maps input {key} of the called graph twice")
            }
            GraphError::SubGraphInputUnmapped { key } => {
                write!(f, "the call does not map input {key} of the called graph")
            }
            GraphError::SubGraphSourceMismatch { key } => write!(
                f,
                "the call maps input {key} from a source that is missing or of another kind"
            ),
            GraphError::SubGraphNotAConfig { key } => write!(
                f,
                "the call maps {key}, which is not a configuration key of the called graph"
            ),
            GraphError::SubGraphConfigTwice { key } => write!(
                f,
                "the call maps configuration key {key} of the called graph twice"
            ),
            GraphError::SubGraphConfigUnmapped { key } => write!(
                f,
                "the call does not map configuration key {key} of the called graph"
            ),
            GraphError::SubGraphConfigMismatch { key } => write!(
                f,
                "the call maps configuration key {key} from a source that is missing or of another kind"
            ),
            GraphError::SubGraphNotAnOutput { key } => write!(
                f,
                "the call maps {key}, which is not a declared output of the called graph"
            ),
            GraphError::SubGraphTargetMismatch { key } => write!(
                f,
                "the call writes into {key}, which is missing or cannot take the value"
            ),
            GraphError::SubGraphSetTwice { key } => {
                write!(f, "the call sets {key} from two outputs")
            }
            GraphError::SubGraphFailed { source } => write!(f, "the called graph failed: {source}"),
            GraphError::SubGraphSuspended => {
                f.write_str("the called graph stopped before its end (paused or cancelled)")
            }
            GraphError::MapBodyNotAppend { key } => write!(
                f,
                "a map body may only append to a list; it returned another update of {key}"
            ),
            GraphError::MapBodySet { key } => {
                write!(f, "a map only appends to lists; {key} would be set")
            }
            GraphError::MapWithoutOccurrence => {
                f.write_str("a map runs as a node of a graph: its context names no node occurrence")
            }
            GraphError::CaptureMismatch { key } => write!(
                f,
                "a failure is captured into {key} from a source that is missing or of a kind {key} cannot take"
            ),
            GraphError::CaptureSetTwice { key } => {
                write!(f, "a failure is captured by setting {key} twice")
            }
            GraphError::InvalidNode { node, source } => {
                write!(f, "node {node} is refused: {source}")
            }
        }
    }
}
