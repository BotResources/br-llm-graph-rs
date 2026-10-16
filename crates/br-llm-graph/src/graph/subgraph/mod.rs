//! A node that runs a graph to its end, as a nested run with private state.

mod check;
mod describe;
mod run;
mod typed;

use std::sync::Arc;

use crate::error::GraphError;
use crate::graph::graph::Graph;
use crate::state::{State, Value};
use crate::update::Update;
use crate::value::Key;

use check::check_call;
pub(crate) use check::check_captures;
pub(crate) use describe::capture_shapes;

/// Where the value of a child input (or child configuration key) comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// A parent state key.
    From(Key),
    /// A parent configuration key.
    Config(Key),
    /// A fixed value.
    Const(Value),
}

/// Where a value a node gives goes in the state of the graph it runs in: an
/// output of a called graph (`SubGraph`), or the answer of an `LlmCall`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// Sets a key of the value's kind.
    Set(Key),
    /// Appends to a list of the value's kind.
    Append(Key),
}

impl Output {
    /// The key written.
    pub fn key(&self) -> &Key {
        match self {
            Output::Set(key) | Output::Append(key) => key,
        }
    }

    pub(crate) fn update(&self, value: Value) -> Update {
        match self {
            Output::Set(key) => Update::Set {
                key: key.clone(),
                value,
            },
            Output::Append(key) => Update::Append {
                key: key.clone(),
                value,
            },
        }
    }
}

/// What a call does when the nested run fails.
#[derive(Debug, Clone, PartialEq)]
pub enum OnFailure {
    /// The node fails with `GraphError::SubGraphFailed`, whose source is the
    /// child's error.
    Propagate,
    /// The node returns these updates instead of its outputs: a captured
    /// failure is a result, not a crash. Inside a map they are forwarded in
    /// item order like outputs, and the item counts as finished. Two of them
    /// that `Set` the same caller key are refused at build
    /// (`GraphError::CaptureSetTwice`).
    Capture(Vec<CaptureUpdate>),
}

/// One update a captured failure makes in the caller's state (of a called
/// graph, or of a map item).
#[derive(Debug, Clone, PartialEq)]
pub enum CaptureUpdate {
    /// Sets a parent key of the source's kind.
    Set(Key, CaptureSource),
    /// Appends to a parent list of the source's kind.
    Append(Key, CaptureSource),
}

impl CaptureUpdate {
    /// Where the capture writes.
    pub fn target(&self) -> Output {
        match self {
            CaptureUpdate::Set(key, _) => Output::Set(key.clone()),
            CaptureUpdate::Append(key, _) => Output::Append(key.clone()),
        }
    }

    /// What it writes.
    pub fn source(&self) -> &CaptureSource {
        match self {
            CaptureUpdate::Set(_, source) | CaptureUpdate::Append(_, source) => source,
        }
    }
}

/// Where the value of a capture update comes from.
#[derive(Debug, Clone, PartialEq)]
pub enum CaptureSource {
    /// A fixed value.
    Const(Value),
    /// A key of the caller's state (in a map body, the item key included).
    From(Key),
    /// The message of the child's failure, as a string.
    Reason,
}

/// The updates `captures` make for a failure whose message is `reason`, `From`
/// sources read from `state`.
pub(crate) fn captured(
    captures: &[CaptureUpdate],
    state: &State,
    reason: &str,
) -> Result<Vec<Update>, GraphError> {
    let mut updates = Vec::with_capacity(captures.len());
    for capture in captures {
        let value = match capture.source() {
            CaptureSource::Const(value) => value.clone(),
            CaptureSource::From(key) => state.value(key)?.clone(),
            CaptureSource::Reason => Value::str(reason),
        };
        updates.push(capture.target().update(value));
    }
    Ok(updates)
}

/// A call of a graph from a node of another graph.
///
/// The child runs from `Graph::start_state` with its declared inputs mapped
/// from the caller, to its end, with a context for the calling node's
/// occurrence. Nothing else crosses: the declared outputs mapped here become
/// the node's updates. The child keeps no memory between two calls.
///
/// A call restarts whole: when it starts, the pending entries below its
/// occurrence are removed, and nothing inside the child is recorded or reused.
/// Only the call's own result can be recorded, by the run or the map that
/// holds it.
///
/// A runner maps typed keys (`input_from`, `config_from`, `output_set`,
/// `CaptureUpdate::set_reason`, ...): each mapping takes keys of one kind, so
/// a wrong kind does not compile. A call described as data maps keys by name
/// (`input`, `config`, `output`, `output_end_label`). Either way the build
/// checks every mapping against the two graphs.
pub struct SubGraph {
    pub(crate) graph: Arc<Graph>,
    pub(crate) inputs: Vec<(Key, Input)>,
    pub(crate) config: Vec<(Key, Input)>,
    pub(crate) outputs: Vec<(Key, Output)>,
    pub(crate) end_label: Option<Output>,
    pub(crate) on_failure: OnFailure,
}

impl SubGraph {
    /// A call of `graph`, nothing mapped yet, `OnFailure::Propagate`.
    pub fn call(graph: Arc<Graph>) -> Self {
        Self {
            graph,
            inputs: Vec::new(),
            config: Vec::new(),
            outputs: Vec::new(),
            end_label: None,
            on_failure: OnFailure::Propagate,
        }
    }

    /// Maps a declared input of the child, by name.
    pub fn input(mut self, child_key: Key, source: Input) -> Self {
        self.inputs.push((child_key, source));
        self
    }

    /// Maps a configuration key of the child, by name.
    pub fn config(mut self, child_key: Key, source: Input) -> Self {
        self.config.push((child_key, source));
        self
    }

    /// Maps a declared output of the child into the parent state, by name.
    pub fn output(mut self, child_key: Key, target: Output) -> Self {
        self.outputs.push((child_key, target));
        self
    }

    /// Writes the label the child ended with, as a string, by name.
    pub fn output_end_label(mut self, target: Output) -> Self {
        self.end_label = Some(target);
        self
    }

    /// What the call does when the child fails.
    pub fn on_failure(mut self, on_failure: OnFailure) -> Self {
        self.on_failure = on_failure;
        self
    }

    /// The called graph.
    pub fn graph(&self) -> &Arc<Graph> {
        &self.graph
    }
}

#[cfg(test)]
mod subgraph_capture_tests;
#[cfg(test)]
mod subgraph_check_tests;
#[cfg(test)]
mod subgraph_hook_tests;
#[cfg(test)]
mod subgraph_map_tests;
#[cfg(test)]
mod subgraph_restart_tests;
#[cfg(test)]
mod subgraph_resume_tests;
#[cfg(test)]
mod subgraph_run_tests;
#[cfg(test)]
mod subgraph_typed_tests;
#[cfg(test)]
mod subgraph_witness_tests;
#[cfg(test)]
mod test_support;
