//! A node that runs a graph to its end, as a nested run with private state.

mod check;
mod run;

use std::sync::Arc;

use crate::graph::graph::Graph;
use crate::state::Value;
use crate::update::Update;
use crate::value::Key;

pub(crate) use check::{Placement, check_call};

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

/// Where a child output goes in the parent state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// Sets a parent key of the output's kind.
    Set(Key),
    /// Appends to a parent list of the output's kind.
    Append(Key),
}

impl Output {
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
}

/// A call of a graph from a node of another graph.
///
/// The child runs from `Graph::start_state` with its declared inputs mapped
/// from the caller, to its end, with a context for the calling node's
/// occurrence. Nothing else crosses: the declared outputs mapped here become
/// the node's updates. The child keeps no memory between two calls.
pub struct SubGraph {
    pub(crate) graph: Arc<Graph>,
    pub(crate) inputs: Vec<(Key, Input)>,
    pub(crate) config: Vec<(Key, Input)>,
    pub(crate) outputs: Vec<(Key, Output)>,
    pub(crate) end_label: Option<Output>,
    pub(crate) on_failure: OnFailure,
}

impl SubGraph {
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

    /// Maps a declared input of the child.
    pub fn input(mut self, child_key: Key, source: Input) -> Self {
        self.inputs.push((child_key, source));
        self
    }

    /// Maps a configuration key of the child.
    pub fn config(mut self, child_key: Key, source: Input) -> Self {
        self.config.push((child_key, source));
        self
    }

    /// Maps a declared output of the child into the parent state.
    pub fn output(mut self, child_key: Key, target: Output) -> Self {
        self.outputs.push((child_key, target));
        self
    }

    /// Writes the label the child ended with, as a string.
    pub fn output_end_label(mut self, target: Output) -> Self {
        self.end_label = Some(target);
        self
    }

    pub fn on_failure(mut self, on_failure: OnFailure) -> Self {
        self.on_failure = on_failure;
        self
    }

    pub fn graph(&self) -> &Arc<Graph> {
        &self.graph
    }
}

#[cfg(test)]
mod subgraph_check_tests;
#[cfg(test)]
mod subgraph_map_tests;
#[cfg(test)]
mod subgraph_resume_tests;
#[cfg(test)]
mod subgraph_run_tests;
#[cfg(test)]
mod test_support;
