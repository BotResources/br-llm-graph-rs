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
mod acceptance;
#[cfg(test)]
mod testkit;

pub mod error;
pub mod graph;
pub mod observe;
pub mod origin;
pub mod react;
pub mod run;
pub mod session;
pub mod state;
pub mod update;
pub mod value;

pub use error::{GraphError, NodeFault};
pub use graph::{
    Always, CaptureSource, CaptureUpdate, Context, Edge, FnEdge, FnNode, Graph, GraphBuilder,
    IdSource, Input, Limit, Map, Node, NodeError, NodeFuture, OnFailure, Output, Signature,
    SubGraph, Target,
};
pub use observe::{NoopObserver, Observer};
pub use origin::{OccurrenceKey, Origin, RunId, Segment};
pub use react::{
    LlmNode, Model, ModelError, ModelFuture, OnLimit, OutputMode, ReactLoop, Request, RoundLimit,
    Source, StreamSink, Tool, ToolCalls, ToolError, ToolFuture, ToolNode, ToolOutput, ToolSpec,
    complete, pending_calls, pending_unsafe_calls, structured, wire,
};
pub use run::{
    Checkpoint, Cursor, Inbox, Outcome, PendingEntry, PendingWrites, RunFailure, Sender, channel,
    run,
};
pub use session::{Ended, Session, Start};
pub use state::{Config, Finite, Kind, SCHEMA_VERSION, Schema, SchemaBuilder, State, Value};
pub use update::Update;
pub use value::{EndLabel, Key, NodeId};
