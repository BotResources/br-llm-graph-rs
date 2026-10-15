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
        clippy::indexing_slicing,
        clippy::result_large_err
    )
)]

#[cfg(test)]
mod acceptance;
#[cfg(doctest)]
mod non_exhaustive;
#[cfg(test)]
mod testkit;
#[cfg(doctest)]
mod typed_kinds;
#[cfg(doctest)]
mod typed_mappings;

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
    Always, CaptureSource, CaptureUpdate, CheckSite, Context, Count, Edge, FnEdge, FnNode, Graph,
    GraphBuilder, IdSource, Input, ItemFailure, Limit, Map, Node, NodeError, NodeFuture, OnFailure,
    Output, Signature, SubGraph, Target,
};
pub use observe::{NoopObserver, Observer};
pub use origin::{OccurrenceKey, Origin, RunId, Segment};
pub use react::{
    AgentNode, Budgeted, CriticSeat, GeneratorCritic, GeneratorSeat, Given, History, Hook,
    HookError, HookFuture, Judgment, LlmCall, LlmOutput, LlmRequest, Model, ModelNode, OnLimit,
    Prompt, ReactAgent, Reads, RequestContext, RoundLimit, Scheduled, Tool, ToolError, ToolFuture,
    ToolOutput, Verdict, VerdictOverride, VerdictReader, complete, pending_calls, structured, wire,
};
pub use run::{
    Checkpoint, Control, ControlSender, Cursor, Outcome, PendingEntry, PendingWrites, RunFailure,
    control, run,
};
pub use session::{Ended, Session, Start};
pub use state::{
    Config, ConfigKey, Finite, Kind, SCHEMA_VERSION, Schema, SchemaBuilder, State, StateKey, Value,
    ValueType,
};
pub use update::Update;
pub use value::{EndLabel, Key, NodeId, ThreadId};
