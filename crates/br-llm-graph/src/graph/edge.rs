//! Edges: what runs after a node, and the targets an edge declares.

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

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Target::Node(id) => write!(f, "node {id}"),
            Target::End(label) => write!(f, "end {label}"),
        }
    }
}

/// The edge of a node: a synchronous function of the state and the
/// configuration, evaluated after the superstep's updates and inputs are
/// applied, that names the next targets. An error fails the run with that
/// error; no target fails it (`EmptyEdge`).
///
/// An edge may declare its targets, known without running it
/// (`Edge::declared`). The graph takes them when it is built: a declared node
/// target it does not hold is refused (`EdgeToUnknownNode`), and a target the
/// edge returns without declaring it fails the run (`UndeclaredTarget`)
/// before any target runs. The description of the graph writes them.
pub trait Edge: Send + Sync {
    /// The targets after the node, read from the new state.
    fn next(&self, state: &State, config: &Config) -> Result<Vec<Target>, GraphError>;

    /// The targets this edge may give, nodes and end labels, in the order
    /// given. `None`, the default: the edge declares none, its targets are
    /// undeclared and checked only when it runs (`UnknownNode`).
    fn declared(&self) -> Option<&[Target]> {
        None
    }
}

/// An edge built from a function. It declares no target unless it is given
/// its targets (`FnEdge::targets`).
pub struct FnEdge<F> {
    f: F,
    declared: Option<Vec<Target>>,
}

impl<F> FnEdge<F>
where
    F: Fn(&State, &Config) -> Result<Vec<Target>, GraphError> + Send + Sync,
{
    /// The edge of `f`, its targets undeclared.
    pub fn new(f: F) -> Self {
        Self { f, declared: None }
    }

    /// This edge, declaring `targets`, in the order given; a target given
    /// twice is declared once, at its first place. A target `f` returns
    /// without declaring it fails the run (`UndeclaredTarget`).
    ///
    /// ```
    /// use br_llm_graph::{Edge, EndLabel, FnEdge, NodeId, Target};
    ///
    /// let retry = Target::Node(NodeId::new("retry").unwrap());
    /// let done = Target::End(EndLabel::new("done").unwrap());
    /// let edge = FnEdge::new(|_, _| Ok(vec![Target::End(EndLabel::new("done").unwrap())]))
    ///     .targets([retry.clone(), done.clone(), retry.clone()]);
    /// assert_eq!(edge.declared(), Some(&[retry, done][..]));
    /// ```
    pub fn targets(mut self, targets: impl IntoIterator<Item = Target>) -> Self {
        let mut declared: Vec<Target> = Vec::new();
        for target in targets {
            if !declared.contains(&target) {
                declared.push(target);
            }
        }
        self.declared = Some(declared);
        self
    }
}

impl<F> Edge for FnEdge<F>
where
    F: Fn(&State, &Config) -> Result<Vec<Target>, GraphError> + Send + Sync,
{
    fn next(&self, state: &State, config: &Config) -> Result<Vec<Target>, GraphError> {
        (self.f)(state, config)
    }

    fn declared(&self) -> Option<&[Target]> {
        self.declared.as_deref()
    }
}

/// An edge that always gives one target, and declares it.
pub struct Always(pub Target);

impl Edge for Always {
    fn next(&self, _state: &State, _config: &Config) -> Result<Vec<Target>, GraphError> {
        Ok(vec![self.0.clone()])
    }

    fn declared(&self) -> Option<&[Target]> {
        Some(std::slice::from_ref(&self.0))
    }
}

#[cfg(test)]
#[path = "edge_tests.rs"]
mod tests;
