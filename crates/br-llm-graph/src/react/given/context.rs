//! The context a function given to a model-calling node reads: what every
//! context holds, and what the context of each kind of node adds.

use br_llm_messages::{Conversation, Step};

use crate::error::GraphError;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::given::nodes::{AgentNode, Budgeted, GeneratorCriticNode, LoopNode, ModelNode};
use crate::react::given::nodes::{CheckFacts, CriticFacts, Scheduled};
use crate::react::given::reads::Reads;
use crate::react::seats::CriticSeat;
use crate::state::{Config, ConfigKey, State, StateKey, ValueType};

/// What a function given to a model-calling node reads to build one request:
/// the context of the node `N`, and only that.
///
/// Every context holds the state and the configuration of the node's run,
/// read through typed keys, and only the keys the function declared with its
/// `Reads`: a key it did not declare is refused (`UnknownKey`), whatever the
/// state holds. The context of each node adds what that node knows:
///
/// - an LLM call's holds nothing more;
/// - an agent's adds its conversation (`AgentNode`), and the round of the
///   ReAct loop for the ReAct agent and the generator (`LoopNode`);
/// - a generator-critic's adds the generation number and the budget
///   (`GeneratorCriticNode`): the generator's, the critic's and the
///   generator-critic's own, which its rule check and its empty-answer
///   critique read with the answer judged (`RequestContext::answer`);
/// - the critic's adds the answer it judges and the attempt number.
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
    /// The agent's conversation, as the node finds it: the conversation of
    /// the agent's graph, the turn under way included.
    pub fn conversation(&self) -> &'a Conversation {
        N::conversation(self.facts)
    }
}

impl<'a, N: LoopNode> RequestContext<'a, N> {
    /// The round of this model call: the number of rounds the turn the reply
    /// goes to has already answered, 0 for the first call of a turn. A round
    /// is one reply that calls tools plus the execution of its calls. The
    /// model is asked once per turn, so its function reads the round of the
    /// turn's first call in this call of the agent: 0, or for a turn the
    /// agent continues in memory, the rounds it had already answered.
    pub fn round(&self) -> usize {
        N::round(self.facts)
    }
}

impl<'a, N: GeneratorCriticNode> RequestContext<'a, N> {
    /// The number of the generation, counted from 1 among the generator's
    /// turns of this call, revisions included: for the generator, the one it
    /// writes; for the critic, the rule check and the empty-answer critique,
    /// the one they judge.
    pub fn generation(&self) -> usize {
        N::count(self.facts).generation
    }

    /// The critique budget of this call, as resolved for this run.
    pub fn budget(&self) -> usize {
        N::count(self.facts).budget
    }
}

impl<'a> RequestContext<'a, CriticSeat> {
    /// The answer the critic judges, as it reads it: the JSON text of its
    /// structured block when it has one, else its text blocks joined
    /// verbatim.
    pub fn answer(&self) -> &'a str {
        let CriticFacts { answer, .. } = self.facts;
        answer
    }

    /// The attempt of this critic call, from 1: one more for each verdict
    /// retry.
    pub fn attempt(&self) -> usize {
        let CriticFacts { attempt, .. } = self.facts;
        attempt
    }
}

impl<'a> RequestContext<'a, GeneratorCritic> {
    /// The final step of the generation judged: its text, its structured
    /// block and its stop reason.
    pub fn answer(&self) -> &'a Step {
        let CheckFacts { answer, .. } = self.facts;
        answer
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
