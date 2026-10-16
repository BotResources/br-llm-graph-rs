//! Graphs: nodes, edges and the builder that checks them; the nodes the library
//! gives (a call of another graph, a map over a list); the bounds read at run
//! time (`Limit`, `Count`); the context a node runs in and the host's id
//! source.

mod builder;
mod context;
mod count;
mod edge;
#[allow(clippy::module_inception)]
mod graph;
mod ids;
mod limit;
mod map;
mod node;
mod private;
mod run_local;
mod signature;
mod subgraph;

pub use builder::GraphBuilder;
pub use context::Context;
pub use count::Count;
pub use edge::{Always, Edge, FnEdge, Target};
pub use graph::Graph;
pub use ids::{IdRequest, IdSource};
pub use limit::Limit;
pub use map::{ItemFailure, Map};
pub use node::{CheckSite, FnNode, Node, NodeError, NodeFuture};
pub use signature::Signature;
pub use subgraph::{CaptureSource, CaptureUpdate, Input, OnFailure, Output, SubGraph};
