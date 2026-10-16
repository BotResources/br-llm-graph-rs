//! Nodes: the asynchronous behaviours of a graph.

use std::future::Future;
use std::pin::Pin;

use crate::describe::{NodeDescription, Realizing};
use crate::error::GraphError;
use crate::graph::context::Context;
use crate::state::{Config, Schema, State};
use crate::update::Update;

/// The error of a node, of any type: it fails the run with `NodeFailed`, its
/// fault `Returned`, where the host can downcast it to its own type.
pub type NodeError = Box<dyn std::error::Error + Send + Sync>;

/// One run of a node under way: its updates, or its error.
pub type NodeFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<Update>, NodeError>> + Send + 'a>>;

/// Where a node is checked: as a node of a graph, or as the body of a map,
/// whose updates may only be appends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CheckSite {
    /// As a node of a graph.
    Graph,
    /// As the body of a map.
    MapBody,
}

/// A behaviour: reads a state snapshot and returns updates.
pub trait Node: Send + Sync {
    /// Runs the node on the state of the superstep, the configuration and its
    /// context, and returns its updates. The nodes of one superstep run
    /// together and read the same state; a panic, while the call starts or
    /// inside its future, is a failure of the node.
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a>;

    /// Checks the node against the schema of the graph it runs in, at
    /// `site`. `GraphBuilder::build` calls it on every node, however
    /// registered, with `CheckSite::Graph`, and refuses the graph with
    /// `GraphError::InvalidNode` when it fails; a `Map` calls it on its body
    /// with `CheckSite::MapBody`. A node that wraps another forwards the call,
    /// site included.
    fn check(&self, schema: &Schema, site: CheckSite) -> Result<(), GraphError> {
        let _ = (schema, site);
        Ok(())
    }

    /// What the node says of itself in its graph's description
    /// (`Graph::describe`): its kind, the fields of its kind and its realizes
    /// labels; the graph adds its id and its edge. It runs nothing and reads
    /// no state. The default: no kind of its own (`node`, or `join` when
    /// registered as one), nothing nested, no label. A host's node may give
    /// its own kind label and nested description
    /// (`NodeDescription::labelled`, `NodeDescription::nesting`). A node that
    /// wraps another forwards the call, as it forwards `check`.
    fn describe(&self) -> NodeDescription {
        NodeDescription::default()
    }

    /// This node, carrying `labels` as its realizes labels: the names of
    /// what it implements, written in the description in the order given and
    /// never read by the library (see `Realizing`).
    fn realizing<L: Into<String>>(self, labels: impl IntoIterator<Item = L>) -> Realizing<Self>
    where
        Self: Sized,
    {
        Realizing::new(self, labels)
    }
}

/// A node built from a function.
pub struct FnNode<F>(F);

impl<F> FnNode<F>
where
    F: for<'a> Fn(&'a State, &'a Config, &'a Context) -> NodeFuture<'a> + Send + Sync,
{
    /// The node of `f`.
    pub fn new(f: F) -> Self {
        Self(f)
    }
}

impl<F> Node for FnNode<F>
where
    F: for<'a> Fn(&'a State, &'a Config, &'a Context) -> NodeFuture<'a> + Send + Sync,
{
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        (self.0)(state, config, ctx)
    }
}
