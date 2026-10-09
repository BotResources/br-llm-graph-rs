use std::future::Future;
use std::pin::Pin;

use crate::error::GraphError;
use crate::graph::context::Context;
use crate::state::{Config, Schema, State};
use crate::update::Update;

pub type NodeError = Box<dyn std::error::Error + Send + Sync>;

pub type NodeFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<Update>, NodeError>> + Send + 'a>>;

/// Where a node is checked: as a node of a graph, or as the body of a map,
/// whose updates may only be appends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CheckSite {
    Graph,
    MapBody,
}

/// A behaviour: reads a state snapshot and returns updates.
pub trait Node: Send + Sync {
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
}

pub struct FnNode<F>(F);

impl<F> FnNode<F>
where
    F: for<'a> Fn(&'a State, &'a Config, &'a Context) -> NodeFuture<'a> + Send + Sync,
{
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
