#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

#[cfg(test)]
mod testkit;

pub mod error;
pub mod graph;
pub mod observe;
pub mod react;
pub mod run;
pub mod session;
pub mod state;
pub mod update;
pub mod value;

pub use error::{GraphError, NodeFault};
pub use graph::{
    Always, Context, Edge, FnEdge, FnNode, Graph, GraphBuilder, IdSource, Map, Node, NodeError,
    NodeFuture, Target,
};
pub use observe::{NoopObserver, Observer};
pub use react::{
    LlmNode, Model, ModelError, ModelFuture, OutputMode, ReactLoop, Request, Source, StreamSink,
    Tool, ToolError, ToolFuture, ToolNode, ToolOutput, ToolSpec, complete, pending_calls,
    pending_unsafe_calls, structured, wire,
};
pub use run::{Checkpoint, Cursor, Inbox, Outcome, RunFailure, Sender, channel, run};
pub use session::{Ended, Session, Start};
pub use state::{Config, Finite, Kind, SCHEMA_VERSION, Schema, SchemaBuilder, State, Value};
pub use update::Update;
pub use value::{EndLabel, Key, NodeId};
