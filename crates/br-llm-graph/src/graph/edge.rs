//! Edges: what runs after a node.

use crate::error::GraphError;
use crate::state::{Config, State};
use crate::value::{EndLabel, NodeId};

/// What an edge names: a node to run in the next superstep, or an end label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A node of the graph; a node the graph does not hold fails the run
    /// (`UnknownNode`).
    Node(NodeId),
    /// An end label: counted only when the superstep ends the run.
    End(EndLabel),
}

/// The edge of a node: a synchronous function of the state and the
/// configuration, evaluated after the superstep's updates and inputs are
/// applied, that names the next targets. An error fails the run with that
/// error; no target fails it (`EmptyEdge`).
pub trait Edge: Send + Sync {
    /// The targets after the node, read from the new state.
    fn next(&self, state: &State, config: &Config) -> Result<Vec<Target>, GraphError>;
}

/// An edge built from a function.
pub struct FnEdge<F>(F);

impl<F> FnEdge<F>
where
    F: Fn(&State, &Config) -> Result<Vec<Target>, GraphError> + Send + Sync,
{
    /// The edge of `f`.
    pub fn new(f: F) -> Self {
        Self(f)
    }
}

impl<F> Edge for FnEdge<F>
where
    F: Fn(&State, &Config) -> Result<Vec<Target>, GraphError> + Send + Sync,
{
    fn next(&self, state: &State, config: &Config) -> Result<Vec<Target>, GraphError> {
        (self.0)(state, config)
    }
}

/// An edge that always gives one target.
pub struct Always(pub Target);

impl Edge for Always {
    fn next(&self, _state: &State, _config: &Config) -> Result<Vec<Target>, GraphError> {
        Ok(vec![self.0.clone()])
    }
}
