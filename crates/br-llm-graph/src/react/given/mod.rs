//! What a runner gives a model-calling node: each subject (its model, its
//! system, its history...) a value, or a function of the node's context.

mod context;
pub(crate) mod facts;
mod nodes;
mod origin;
mod reads;
mod ready;

use std::sync::Arc;

use br_llm_clients_core::Client;
use br_llm_messages::WireMessage;

use crate::graph::NodeError;
use crate::state::{Config, State};

pub use context::RequestContext;
pub use nodes::{AgentNode, Budgeted, GeneratorCriticNode, LoopNode, ModelNode, Scheduled};
pub(crate) use nodes::{CallFacts, CheckFacts, CriticFacts, SeatCount};
pub(crate) use origin::Origin;
pub use reads::Reads;

/// A function given to a model-calling node: it reads the node's context and
/// gives one subject of the request, or an error that fails the node as
/// returned.
type GiveFn<N, T> = dyn for<'a> Fn(&RequestContext<'a, N>) -> Result<T, NodeError> + Send + Sync;

/// One subject of a request the node `N` sends, of the type `T`: a value the
/// runner gives when the node is built, or a function the runner writes.
///
/// A node that calls a model (`LlmCall`, `ReactAgent`, each seat of
/// `GeneratorCritic`) is built from what the runner gives it, one subject at
/// a time, each of a fixed type: the model gives a client (`Model`), the
/// system a text (`Prompt`), the history the conversation to send
/// (`History`), the reminder of a reply asked for again a text (`Prompt`).
///
/// A function:
/// - reads the context of its node (`RequestContext`), and only that: the
///   state and the configuration through typed keys, and only the keys
///   declared with it in its `Reads`, and the attempt number; an agent's
///   context adds its conversation, and more for some nodes (see
///   `RequestContext`);
/// - is synchronous and does no I/O: what it needs for every run (prompts,
///   clients, catalogues) is given to it when the node is built, typed by the
///   runner, as what the closure captures; it never goes through the state.
///   What changes from run to run is in the state;
/// - shapes the request its node sends, never what is recorded, so it cannot
///   break a resumption;
/// - fails its node with its error, as returned (`NodeFault::Returned`,
///   where the host can downcast it).
///
/// The common values are ready-made: a fixed text and a text read from a
/// key (`Given::text`, `Given::from_state`...), a fixed client
/// (`Given::client`) and schedules of clients (`Given::schedule`, and
/// `Given::ending_with` for the two seats of the generator-critic).
///
/// The system text of a ReAct agent, from a catalogue the runner holds and
/// the state key `topic`, which becomes an input of the agent's graph:
///
/// ```
/// use std::sync::Arc;
/// use br_llm_graph::{Prompt, ReactAgent, Reads, StateKey};
///
/// let catalogue = Arc::new(vec!["search".to_owned(), "fetch".to_owned()]);
/// let mut reads = Reads::new();
/// let topic: StateKey<String> = reads.state("topic").unwrap();
/// let system: Prompt<ReactAgent> = Prompt::from_fn(reads, move |ctx| {
///     Ok(format!(
///         "Research {}. Tools: {}. Round {}.",
///         ctx.get(&topic)?,
///         catalogue.join(", "),
///         ctx.round()
///     ))
/// });
/// ```
pub struct Given<N: ModelNode, T> {
    reads: Reads,
    give: Arc<GiveFn<N, T>>,
    /// How it was given, for the graph's description.
    origin: Origin,
}

/// The model of a node: the client it calls, given or chosen per call. A
/// change of client never changes the node's author.
pub type Model<N> = Given<N, Arc<dyn Client>>;

/// A text of a request, such as the system text.
pub type Prompt<N> = Given<N, String>;

/// The conversation a request sends, as wire messages: by default the node
/// renders its own (see each node).
pub type History<N> = Given<N, Vec<WireMessage>>;

impl<N: ModelNode, T: Clone + Send + Sync + 'static> Given<N, T> {
    /// The fixed value `value`, given on every call; it reads no key.
    pub fn value(value: T) -> Self {
        Self::from_fn(Reads::new(), move |_| Ok(value.clone())).with_origin(Origin::Fixed)
    }
}

impl<N: ModelNode, T> Given<N, T> {
    /// The function `give`, reading the keys declared in `reads`.
    pub fn from_fn<F>(reads: Reads, give: F) -> Self
    where
        F: for<'a> Fn(&RequestContext<'a, N>) -> Result<T, NodeError> + Send + Sync + 'static,
    {
        Self {
            reads,
            give: Arc::new(give),
            origin: Origin::Function,
        }
    }

    /// A ready-made subject of the library: `give`, reading the keys
    /// declared in `reads`.
    pub(crate) fn ready_made<F>(reads: Reads, give: F) -> Self
    where
        F: for<'a> Fn(&RequestContext<'a, N>) -> Result<T, NodeError> + Send + Sync + 'static,
    {
        Self::from_fn(reads, give).with_origin(Origin::ReadyMade)
    }

    /// This subject, given as `origin` says.
    pub(crate) fn with_origin(mut self, origin: Origin) -> Self {
        self.origin = origin;
        self
    }

    /// How it was given: a fixed value, a ready-made value or a function.
    pub(crate) fn origin(&self) -> Origin {
        self.origin
    }

    /// The keys this subject reads.
    pub fn reads(&self) -> &Reads {
        &self.reads
    }

    /// The subject for one call of the node, its context made of `state`,
    /// `config` and the node's `facts`.
    pub(crate) fn give<'a>(
        &'a self,
        state: &'a State,
        config: &'a Config,
        facts: N::Facts<'a>,
    ) -> Result<T, NodeError> {
        (self.give)(&RequestContext::new(state, config, &self.reads, facts))
    }
}

impl<N: ModelNode, T> Clone for Given<N, T> {
    fn clone(&self) -> Self {
        Self {
            reads: self.reads.clone(),
            give: self.give.clone(),
            origin: self.origin,
        }
    }
}
