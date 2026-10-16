//! What each kind of node holds in a description: the shapes the library's
//! own nodes give, and the shape of a host's node. Crate-private, so a host
//! can give only a host shape and never pass for one of the library's
//! kinds.

use br_llm_messages::Author;

use crate::describe::GraphDescription;
use crate::graph::{Count, Limit, Output};
use crate::react::{Origin, RoundLimit};
use crate::state::Kind;
use crate::value::Key;

/// The kind of a node and the fields of its kind.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Shape {
    /// A node a host writes: `node` or `join` as registered, unless it gives
    /// its own kind label; with the nested description it gives, if any.
    Host {
        label: Option<String>,
        nested: Option<Box<GraphDescription>>,
    },
    /// A call of another graph (`call`), or of an agent's graph
    /// (`react_agent`, `generator_critic`).
    Call(Box<CallShape>),
    /// A map over a list (`map`).
    Map(Box<MapShape>),
    /// An `LlmCall` (`llm_call`).
    LlmCall(LlmCallShape),
    /// The model-call step of an agent, or the critic's model call
    /// (`model_call`).
    ModelCall(Box<ModelCallShape>),
    /// A tool node of an agent (`tools`).
    Tools(ToolsShape),
    /// The check of a generator-critic: a plain node (`node`) holding what
    /// it reads.
    Check(CheckShape),
}

impl Default for Shape {
    fn default() -> Self {
        Shape::Host {
            label: None,
            nested: None,
        }
    }
}

/// The agent whose graph a call runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentKind {
    ReactAgent,
    GeneratorCritic,
}

/// Where a call's input or configuration key, or a capture, takes its value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Source {
    /// A state key of the caller.
    From(Key),
    /// A configuration key of the caller.
    Config(Key),
    /// A fixed value, written as its kind only, never the value; `None` for
    /// an empty list, whose element kind the value does not say.
    Const(Option<Kind>),
    /// The message of the failure (a capture only).
    Reason,
}

/// One capture update: where it writes, and what.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CaptureShape {
    pub(crate) target: Output,
    pub(crate) source: Source,
}

/// A call: its mappings, its failure policy and the graph it runs.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CallShape {
    pub(crate) agent: Option<AgentKind>,
    pub(crate) inputs: Vec<(Key, Source)>,
    pub(crate) config: Vec<(Key, Source)>,
    pub(crate) outputs: Vec<(Key, Output)>,
    pub(crate) end_label: Option<Output>,
    /// `None`: `Propagate`; the captures of `Capture` otherwise.
    pub(crate) captures: Option<Vec<CaptureShape>>,
    pub(crate) graph: GraphDescription,
}

/// What a map does on an item error.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ItemPolicy {
    Finish,
    FailFast,
    Capture(Vec<CaptureShape>),
}

/// A map: its keys, its limit, its policy and its body.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MapShape {
    pub(crate) list: Key,
    pub(crate) item: Key,
    pub(crate) max_concurrency: Option<Limit>,
    pub(crate) on_item_failure: ItemPolicy,
    pub(crate) body: crate::describe::NodeDescription,
}

/// Who speaks, by its id and its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuthorShape {
    pub(crate) id: String,
    pub(crate) name: String,
}

impl AuthorShape {
    pub(crate) fn of(author: &Author) -> Self {
        Self {
            id: author.id().as_str().to_owned(),
            name: author.name().as_str().to_owned(),
        }
    }
}

/// What an `LlmCall` sends as its request.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RequestShape {
    Text(Origin),
    Conversation { key: Key, author: AuthorShape },
    History(Origin),
}

/// An `LlmCall`: how its subjects are given, its request and its output.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LlmCallShape {
    pub(crate) model: Origin,
    pub(crate) system: Origin,
    pub(crate) request: RequestShape,
    pub(crate) output: Output,
}

/// A model-calling node of an agent: its seat, its tools and its bounds;
/// for the critic, what the generator-critic's routing and verdict read.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ModelCallShape {
    pub(crate) author: AuthorShape,
    pub(crate) model: Origin,
    pub(crate) system: Origin,
    pub(crate) history: Origin,
    pub(crate) tools: Vec<String>,
    pub(crate) enabled: Option<Key>,
    pub(crate) tool_concurrency: Option<Limit>,
    pub(crate) round_limit: Option<RoundLimit>,
    pub(crate) critic: Option<CriticShape>,
}

/// What the critic's node of a generator-critic holds beyond its seat.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CriticShape {
    pub(crate) max_critiques: Count,
    pub(crate) final_generation: bool,
    pub(crate) verdict_retries: Count,
    pub(crate) review_request: Origin,
    /// Whether the verdict has the default shape or the caller's.
    pub(crate) verdict_override: bool,
}

/// A tool node: the names of the tools it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolsShape {
    pub(crate) tools: Vec<String>,
}

/// The check of a generator-critic: what it reads.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CheckShape {
    pub(crate) revision_limit: Count,
    pub(crate) rule_check: Option<Origin>,
    pub(crate) empty_answer_critique: Origin,
}

impl Shape {
    /// The kind written for a node of this shape registered as a join
    /// (`is_join`) or not.
    pub(crate) fn kind(&self, is_join: bool) -> &str {
        match self {
            Shape::Host {
                label: Some(label), ..
            } => label,
            Shape::Host { label: None, .. } if is_join => "join",
            Shape::Host { label: None, .. } | Shape::Check(_) => "node",
            Shape::Call(call) => match call.agent {
                None => "call",
                Some(AgentKind::ReactAgent) => "react_agent",
                Some(AgentKind::GeneratorCritic) => "generator_critic",
            },
            Shape::Map(_) => "map",
            Shape::LlmCall(_) => "llm_call",
            Shape::ModelCall(_) => "model_call",
            Shape::Tools(_) => "tools",
        }
    }

    /// Whether a join registration is written as `"join": true`: for every
    /// shape whose kind is not already `join`.
    pub(crate) fn marks_join(&self) -> bool {
        !matches!(self, Shape::Host { label: None, .. })
    }
}

/// The kinds the library writes, which no host label may take.
pub(crate) const LIBRARY_KINDS: [&str; 9] = [
    "node",
    "join",
    "call",
    "map",
    "llm_call",
    "react_agent",
    "generator_critic",
    "tools",
    "model_call",
];
