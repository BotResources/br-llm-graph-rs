//! The context a function given to a model-calling node reads, one per node:
//! what it holds, and the nodes it belongs to.

use br_llm_messages::Conversation;

use crate::error::GraphError;
use crate::react::given::reads::Reads;
use crate::react::llm_call::LlmCall;
use crate::react::react_agent::ReactAgent;
use crate::react::seats::{CriticSeat, GeneratorSeat};
use crate::state::{Config, ConfigKey, State, StateKey, ValueType};

/// What a function given to a model-calling node reads to build one request:
/// the context of the node `N`, and only that.
///
/// Every context holds the state and the configuration of the node's run,
/// read through typed keys, and only the keys the function declared with its
/// `Reads`: a key it did not declare is refused (`UnknownKey`), whatever the
/// state holds. The context of an agent adds its conversation and the round
/// (`AgentNode`).
pub struct RequestContext<'a, N: ModelNode> {
    state: &'a State,
    config: &'a Config,
    reads: &'a Reads,
    facts: N::Facts<'a>,
}

impl<'a, N: ModelNode> RequestContext<'a, N> {
    /// The context of a function that declared `reads`.
    pub(crate) fn new(
        state: &'a State,
        config: &'a Config,
        reads: &'a Reads,
        facts: N::Facts<'a>,
    ) -> Self {
        Self {
            state,
            config,
            reads,
            facts,
        }
    }

    /// The value of the state key `key`, as its type. A key the function did
    /// not declare is refused (`UnknownKey`); then the refusals of
    /// `State::get`.
    pub fn get<T: ValueType>(&self, key: &StateKey<T>) -> Result<T::Read<'a>, GraphError> {
        if !self.reads.reads_state(key.key()) {
            return Err(GraphError::UnknownKey {
                key: key.key().clone(),
            });
        }
        self.state.get(key)
    }

    /// The value of the configuration key `key`, as its type. A key the
    /// function did not declare is refused (`UnknownKey`); then the refusals
    /// of `Config::get`.
    pub fn config<T: ValueType>(&self, key: &ConfigKey<T>) -> Result<T::Read<'a>, GraphError> {
        if !self.reads.reads_config(key.key()) {
            return Err(GraphError::UnknownKey {
                key: key.key().clone(),
            });
        }
        self.config.get(key)
    }
}

impl<'a, N: AgentNode> RequestContext<'a, N> {
    /// The agent's conversation, as the model call finds it: the conversation
    /// of the agent's graph, its last turn included.
    pub fn conversation(&self) -> &'a Conversation {
        N::agent(self.facts).conversation
    }

    /// The round of this model call: the number of rounds the agent's open
    /// turn has already answered, 0 for the first call of a turn. A round is
    /// one reply that calls tools plus the execution of its calls.
    pub fn round(&self) -> usize {
        N::agent(self.facts).round
    }
}

impl<'a, N: Scheduled> RequestContext<'a, N> {
    /// The 0-based index of this call in the node's schedule of clients.
    pub(crate) fn call_index(&self) -> usize {
        N::call_index(self.facts)
    }
}

impl<'a, N: Budgeted> RequestContext<'a, N> {
    /// The number of calls the node's budget allows.
    pub(crate) fn calls_allowed(&self) -> usize {
        N::calls_allowed(self.facts)
    }
}

/// What a context holds beyond the state and the configuration. Its types
/// are public for the trait's sake, never reachable by name.
pub(crate) mod facts {
    use br_llm_messages::Conversation;

    /// What an agent's context adds: its conversation and the round.
    #[derive(Clone, Copy)]
    pub struct AgentFacts<'a> {
        pub(crate) conversation: &'a Conversation,
        pub(crate) round: usize,
    }
}

pub(crate) use facts::AgentFacts;

mod sealed {
    pub trait Sealed {}

    impl Sealed for super::LlmCall {}
    impl Sealed for super::ReactAgent {}
    impl Sealed for super::GeneratorSeat {}
    impl Sealed for super::CriticSeat {}
}

/// A node that calls a model, whose subjects (model, system, history...)
/// may be functions of its `RequestContext`: the LLM call, the ReAct agent
/// and each seat of the generator-critic. Sealed: only they implement it.
pub trait ModelNode: sealed::Sealed + 'static {
    /// What the node's context holds beyond the state and the configuration.
    #[doc(hidden)]
    type Facts<'a>: Copy + Send + Sync;
}

/// A node whose context is an agent's: it adds the agent's conversation and
/// the round (`RequestContext::conversation`, `RequestContext::round`).
pub trait AgentNode: ModelNode {
    #[doc(hidden)]
    fn agent<'a>(facts: Self::Facts<'a>) -> AgentFacts<'a>;

    #[doc(hidden)]
    fn from_agent<'a>(agent: AgentFacts<'a>) -> Self::Facts<'a>;
}

/// A node whose calls follow one another, so a schedule of clients from the
/// start (`Given::schedule`) knows the index of each call. For the ReAct
/// agent, a call's index is its round.
pub trait Scheduled: ModelNode {
    #[doc(hidden)]
    fn call_index<'a>(facts: Self::Facts<'a>) -> usize;
}

/// A node whose context holds a budget, so a schedule ending with a tail
/// (`Given::ending_with`) knows the last calls the budget allows.
pub trait Budgeted: Scheduled {
    #[doc(hidden)]
    fn calls_allowed<'a>(facts: Self::Facts<'a>) -> usize;
}

impl ModelNode for LlmCall {
    type Facts<'a> = ();
}

impl ModelNode for ReactAgent {
    type Facts<'a> = AgentFacts<'a>;
}

impl AgentNode for ReactAgent {
    fn agent<'a>(facts: Self::Facts<'a>) -> AgentFacts<'a> {
        facts
    }

    fn from_agent<'a>(agent: AgentFacts<'a>) -> Self::Facts<'a> {
        agent
    }
}

impl Scheduled for ReactAgent {
    fn call_index<'a>(facts: Self::Facts<'a>) -> usize {
        facts.round
    }
}

impl ModelNode for GeneratorSeat {
    type Facts<'a> = AgentFacts<'a>;
}

impl AgentNode for GeneratorSeat {
    fn agent<'a>(facts: Self::Facts<'a>) -> AgentFacts<'a> {
        facts
    }

    fn from_agent<'a>(agent: AgentFacts<'a>) -> Self::Facts<'a> {
        agent
    }
}

impl ModelNode for CriticSeat {
    type Facts<'a> = ();
}
