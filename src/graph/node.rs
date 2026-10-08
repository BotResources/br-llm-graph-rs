use std::any::Any;
use std::future::Future;
use std::pin::Pin;

use crate::graph::context::Context;
use crate::state::{Config, State};
use crate::update::Update;

pub type NodeError = Box<dyn std::error::Error + Send + Sync>;

pub type NodeFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<Update>, NodeError>> + Send + 'a>>;

/// A behaviour: reads a state snapshot and returns updates.
///
/// `Any` lets the graph builder recognise the nodes it checks at build time
/// (`Map`, `SubGraph`); every node a graph holds is `'static` anyway.
pub trait Node: Any + Send + Sync {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a>;
}

pub struct FnNode<F>(F);

impl<F> FnNode<F>
where
    F: for<'a> Fn(&'a State, &'a Config, &'a Context) -> NodeFuture<'a> + Send + Sync + 'static,
{
    pub fn new(f: F) -> Self {
        Self(f)
    }
}

impl<F> Node for FnNode<F>
where
    F: for<'a> Fn(&'a State, &'a Config, &'a Context) -> NodeFuture<'a> + Send + Sync + 'static,
{
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        (self.0)(state, config, ctx)
    }
}
