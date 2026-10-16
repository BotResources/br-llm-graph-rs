//! Agent workflows as graphs over a typed state, on br-llm-messages and
//! br-llm-clients-core.
//!
//! Nodes read a snapshot of the state and return updates; edges read the
//! state and name what runs next; the run advances in supersteps until it
//! reaches an end. A graph runs inside another (`SubGraph`) or once per item
//! of a list (`Map`); an interrupted run resumes without running finished
//! work again; an `Observer` hears what happens.
//!
//! # State, configuration and typed keys
//!
//! A `Schema` declares the state keys, the configuration keys and their
//! kinds (`int`, `float`, `str`, `bool`, `list<element>`, `conversation`).
//! Declaring a key on a `SchemaBuilder` gives a typed key, `StateKey<T>` or
//! `ConfigKey<T>`, `T` the Rust type of its kind (`i64`, `f64`, `String`,
//! `bool`, `Vec<T>`, `Conversation`): reading through it gives a `T`, a
//! `Set` through it takes only a `T`, an `Append` only an element, and a
//! mapping between two typed keys only keys of one kind, so a wrong kind
//! does not compile. Reading and updating by name (`Key`, `Value`, `Update`)
//! stay for what is known only at run time: a state, a configuration or a
//! checkpoint reloaded from JSON, and a graph described as data.
//!
//! # Runs, the control channel and resumption
//!
//! `run` takes a graph, a configuration, a state, an optional cursor, a
//! `Context` and the receiving end of a control channel (`control()`): the
//! host sends inputs, pause, resume and cancel through the `ControlSender`.
//! It gives an `Outcome` (`Finished`, `Paused`, `Cancelled`) or a
//! `RunFailure`, each with a `Checkpoint`: the state, the cursor and the
//! pending entries of the nodes that finished in the open superstep, so a
//! resumed run runs only what did not finish. A `Session` keeps a graph, a
//! configuration, a state and a control channel, runs, relaunches after an
//! end and obeys the host's commands.
//!
//! # Agents
//!
//! The `react` module holds the parts of an agent: an `LlmCall`, a node that
//! makes one model call and gives back its answer as data; a `ReactAgent`;
//! a `GeneratorCritic`. The model is a `Client` of br-llm-clients-core the
//! host gives; the tools (`Tool`, judged by an optional `Hook`) and the
//! thread store (`ThreadStore`) are ports the host implements.
//!
//! A runner gives a model-calling node its model, its system and its
//! history (`Model`, `Prompt`, `History`), each a value or a function of the
//! node's context (`Given`, `RequestContext`), declaring the keys it reads
//! (`Reads`). An agent takes `opening`, what the caller adds before it
//! answers, and keeps its own conversation in a private key, never part of a
//! checkpoint. Built with a thread store it is durable: it works in the
//! thread its input `thread` names (the empty string asks the store to
//! create one), writes every step there first, resumes from it after an
//! interruption and gives the thread's id back. Without one it works in
//! memory, for the current call only. The host's `IdSource` gives every
//! thread id and turn id, the same id for the same request.
//!
//! # Errors
//!
//! One error type, `GraphError`, with a variant per refused rule, and the
//! fault of a failed node, `NodeFault`.
//!
//! # Example
//!
//! A graph of two nodes over typed keys: `draft` writes a text, `count`
//! counts its words.
//!
//! ```
//! use std::sync::Arc;
//!
//! use br_llm_graph::{
//!     Always, Config, Context, EndLabel, FnNode, GraphBuilder, IdRequest, IdSource, NodeFuture,
//!     NodeId, NoopObserver, Outcome, Schema, State, Target, ThreadId, control, run,
//! };
//! use br_llm_messages::TurnId;
//!
//! /// This graph holds no agent: nothing asks it for an id.
//! struct NoAgent;
//!
//! impl IdSource for NoAgent {
//!     fn thread_id(&self, _request: &IdRequest) -> ThreadId {
//!         unreachable!("no agent asks for a thread id")
//!     }
//!     fn turn_id(&self, _request: &IdRequest) -> TurnId {
//!         unreachable!("no agent asks for a turn id")
//!     }
//! }
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! let mut schema = Schema::builder();
//! let topic = schema.state::<String>("topic").unwrap();
//! let text = schema.state::<String>("text").unwrap();
//! let words = schema.state::<i64>("words").unwrap();
//! let schema = schema.build();
//!
//! let (read_topic, write_text) = (topic.clone(), text.clone());
//! let draft = FnNode::new(move |state: &State, _: &Config, _: &Context| -> NodeFuture<'_> {
//!     let update = state
//!         .get(&read_topic)
//!         .map(|topic| format!("A note about {topic}."))
//!         .and_then(|note| write_text.set(note));
//!     Box::pin(async move { Ok(vec![update?]) })
//! });
//! let (read_text, write_words) = (text.clone(), words.clone());
//! let count = FnNode::new(move |state: &State, _: &Config, _: &Context| -> NodeFuture<'_> {
//!     let update = state
//!         .get(&read_text)
//!         .map(|text| text.split_whitespace().count() as i64)
//!         .and_then(|n| write_words.set(n));
//!     Box::pin(async move { Ok(vec![update?]) })
//! });
//!
//! let id = |name: &str| NodeId::new(name).unwrap();
//! let graph = GraphBuilder::new(schema.clone())
//!     .entry(id("draft"))
//!     .node(id("draft"), draft)
//!     .node(id("count"), count)
//!     .edge(id("draft"), Always(Target::Node(id("count"))))
//!     .edge(id("count"), Always(Target::End(EndLabel::new("done").unwrap())))
//!     .input(&topic)
//!     .output(&words)
//!     .build()
//!     .unwrap();
//!
//! let state = graph.start_state([topic.given("graphs").unwrap()]).unwrap();
//! let config = Config::new(&schema, Default::default()).unwrap();
//! let context = Context::new(Arc::new(NoopObserver), Arc::new(NoAgent));
//! let (_sender, mut channel) = control();
//! let Ok(Outcome::Finished { state, end }) =
//!     run(&graph, &config, state, None, &context, &mut channel).await
//! else {
//!     panic!("the run finishes");
//! };
//! assert_eq!(end.as_str(), "done");
//! assert_eq!(state.get(&words).unwrap(), 4);
//! # }
//! ```

#![warn(missing_docs)]
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
mod describe_write_only;
#[cfg(doctest)]
mod non_exhaustive;
#[cfg(doctest)]
mod tails;
#[cfg(test)]
mod testkit;
#[cfg(doctest)]
mod typed_call_mappings;
#[cfg(doctest)]
mod typed_captures;
#[cfg(doctest)]
mod typed_kinds;
#[cfg(doctest)]
mod typed_mappings;

pub mod describe;
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

pub use describe::{DESCRIPTION_VERSION, GraphDescription, NodeDescription, Realizing};
pub use error::{GraphError, NodeFault};
pub use graph::{
    Always, CaptureSource, CaptureUpdate, CheckSite, Context, Count, Edge, FnEdge, FnNode, Graph,
    GraphBuilder, IdRequest, IdSource, Input, ItemFailure, Limit, Map, Node, NodeError, NodeFuture,
    OnFailure, Output, Signature, SubGraph, Target,
};
pub use observe::{NoopObserver, Observer};
pub use origin::{OccurrenceKey, Origin, RunId, Segment};
pub use react::{
    AgentNode, Budgeted, CriticHistory, CriticSeat, GeneratorCritic, GeneratorCriticNode,
    GeneratorSeat, Given, History, Hook, HookError, HookFuture, Judgment, LlmCall, LlmOutput,
    LlmRequest, LoopNode, MessageStatus, Model, ModelNode, OnLimit, Prompt, ReactAgent, Reads,
    RequestContext, RoundLimit, RuleCheck, Ruling, Scheduled, ThreadError, ThreadFuture,
    ThreadMessage, ThreadStore, ThreadView, Tool, ToolError, ToolFuture, ToolOutput, TurnEnd,
    Verdict, VerdictOverride, VerdictReader, complete, pending_calls, structured, wire,
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
