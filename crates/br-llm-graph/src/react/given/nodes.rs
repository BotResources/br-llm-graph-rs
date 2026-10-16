//! The nodes that call a model, sealed, and what the context of each holds
//! beyond the state and the configuration.

use br_llm_messages::Conversation;

use crate::react::generator_critic::GeneratorCritic;
use crate::react::llm_call::LlmCall;
use crate::react::react_agent::ReactAgent;
use crate::react::seats::{CriticSeat, GeneratorSeat};

/// What a context holds beyond the state and the configuration. Its types
/// are public for the traits' sake, never reachable by name.
pub(crate) mod facts {
    use br_llm_messages::{Conversation, Step};

    /// What a ReAct agent's context adds: its conversation and the round;
    /// and, for its schedule, the 0-based index of its turn in the call.
    #[derive(Clone, Copy)]
    pub struct AgentFacts<'a> {
        pub(crate) conversation: &'a Conversation,
        pub(crate) round: usize,
        pub(crate) turn: usize,
    }

    /// Where a generator-critic's call stands: the generation, the budget,
    /// the critiques recorded and whether a final generation follows the
    /// last one.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct SeatCount {
        pub(crate) generation: usize,
        pub(crate) budget: usize,
        pub(crate) critiques: usize,
        pub(crate) final_generation: bool,
    }

    /// What the generator's context adds: its conversation, the round and
    /// where the call stands.
    #[derive(Clone, Copy)]
    pub struct GeneratorFacts<'a> {
        pub(crate) conversation: &'a Conversation,
        pub(crate) round: usize,
        pub(crate) count: SeatCount,
    }

    /// What the critic's context adds: the conversation, where the call
    /// stands, the answer judged and the attempt.
    #[derive(Clone, Copy)]
    pub struct CriticFacts<'a> {
        pub(crate) conversation: &'a Conversation,
        pub(crate) count: SeatCount,
        pub(crate) answer: &'a str,
        pub(crate) attempt: usize,
    }

    /// What the generator-critic's own context adds (its rule check, its
    /// empty-answer critique): the conversation, where the call stands and
    /// the final step judged.
    #[derive(Clone, Copy)]
    pub struct CheckFacts<'a> {
        pub(crate) conversation: &'a Conversation,
        pub(crate) count: SeatCount,
        pub(crate) answer: &'a Step,
    }
}

pub(crate) use facts::{AgentFacts, CheckFacts, CriticFacts, GeneratorFacts, SeatCount};

mod sealed {
    pub trait Sealed {}

    impl Sealed for super::LlmCall {}
    impl Sealed for super::ReactAgent {}
    impl Sealed for super::GeneratorSeat {}
    impl Sealed for super::CriticSeat {}
    impl Sealed for super::GeneratorCritic {}
}

/// A node that calls a model, whose subjects (model, system, history...)
/// may be functions of its `RequestContext`: the LLM call, the ReAct agent,
/// each seat of the generator-critic, and the generator-critic itself for
/// what it gives of its own (its rule check, its empty-answer critique).
/// Sealed: only they implement it.
pub trait ModelNode: sealed::Sealed + 'static {
    /// What the node's context holds beyond the state and the configuration.
    #[doc(hidden)]
    type Facts<'a>: Copy + Send + Sync;
}

/// A node of an agent: its context adds the agent's conversation
/// (`RequestContext::conversation`).
pub trait AgentNode: ModelNode {
    #[doc(hidden)]
    fn conversation<'a>(facts: Self::Facts<'a>) -> &'a Conversation;
}

/// A node that runs the ReAct loop, the ReAct agent and the generator: its
/// context adds the round (`RequestContext::round`).
pub trait LoopNode: AgentNode {
    #[doc(hidden)]
    fn round<'a>(facts: Self::Facts<'a>) -> usize;

    #[doc(hidden)]
    fn loop_facts<'a>(
        conversation: &'a Conversation,
        round: usize,
        turn: usize,
        count: SeatCount,
    ) -> Self::Facts<'a>;
}

/// A node of the generator-critic: its context adds the generation number
/// and the budget (`RequestContext::generation`, `RequestContext::budget`).
pub trait GeneratorCriticNode: AgentNode {
    #[doc(hidden)]
    fn count<'a>(facts: Self::Facts<'a>) -> SeatCount;
}

/// A node whose calls follow one another, so a schedule of clients from the
/// start (`Given::schedule`) knows the index of each call. The model of an
/// agent is asked once per turn, at its first round, and its client kept for
/// the turn's other rounds. For the ReAct agent, a call's index is the index
/// of its turn among the call's turns (a turn opened after a repaired one
/// gets the next client); for each seat of the generator-critic, the number
/// of critiques recorded before it, so a revision or a verdict retry keeps
/// the client of its call.
pub trait Scheduled: ModelNode {
    #[doc(hidden)]
    fn call_index<'a>(facts: Self::Facts<'a>) -> usize;
}

/// A node whose context holds a budget, so a schedule ending with a tail
/// (`Given::ending_with`) knows the last calls the budget allows: the two
/// seats of the generator-critic, and only they. The generator's calls are
/// its generations, revisions not counted: the budget plus one with
/// `final_generation`, else the budget, and one for a budget of 0. The
/// critic's calls are the budget, verdict retries not counted.
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
    fn conversation<'a>(facts: Self::Facts<'a>) -> &'a Conversation {
        facts.conversation
    }
}

impl LoopNode for ReactAgent {
    fn round<'a>(facts: Self::Facts<'a>) -> usize {
        facts.round
    }

    fn loop_facts<'a>(
        conversation: &'a Conversation,
        round: usize,
        turn: usize,
        _count: SeatCount,
    ) -> Self::Facts<'a> {
        AgentFacts {
            conversation,
            round,
            turn,
        }
    }
}

impl Scheduled for ReactAgent {
    fn call_index<'a>(facts: Self::Facts<'a>) -> usize {
        facts.turn
    }
}

impl ModelNode for GeneratorSeat {
    type Facts<'a> = GeneratorFacts<'a>;
}

impl AgentNode for GeneratorSeat {
    fn conversation<'a>(facts: Self::Facts<'a>) -> &'a Conversation {
        facts.conversation
    }
}

impl LoopNode for GeneratorSeat {
    fn round<'a>(facts: Self::Facts<'a>) -> usize {
        facts.round
    }

    fn loop_facts<'a>(
        conversation: &'a Conversation,
        round: usize,
        _turn: usize,
        count: SeatCount,
    ) -> Self::Facts<'a> {
        GeneratorFacts {
            conversation,
            round,
            count,
        }
    }
}

impl GeneratorCriticNode for GeneratorSeat {
    fn count<'a>(facts: Self::Facts<'a>) -> SeatCount {
        facts.count
    }
}

impl Scheduled for GeneratorSeat {
    fn call_index<'a>(facts: Self::Facts<'a>) -> usize {
        facts.count.critiques
    }
}

impl Budgeted for GeneratorSeat {
    fn calls_allowed<'a>(facts: Self::Facts<'a>) -> usize {
        let SeatCount {
            budget,
            final_generation,
            ..
        } = facts.count;
        if final_generation {
            budget + 1
        } else {
            budget.max(1)
        }
    }
}

impl ModelNode for CriticSeat {
    type Facts<'a> = CriticFacts<'a>;
}

impl AgentNode for CriticSeat {
    fn conversation<'a>(facts: Self::Facts<'a>) -> &'a Conversation {
        facts.conversation
    }
}

impl GeneratorCriticNode for CriticSeat {
    fn count<'a>(facts: Self::Facts<'a>) -> SeatCount {
        facts.count
    }
}

impl Scheduled for CriticSeat {
    fn call_index<'a>(facts: Self::Facts<'a>) -> usize {
        facts.count.critiques
    }
}

impl Budgeted for CriticSeat {
    fn calls_allowed<'a>(facts: Self::Facts<'a>) -> usize {
        facts.count.budget
    }
}

impl ModelNode for GeneratorCritic {
    type Facts<'a> = CheckFacts<'a>;
}

impl AgentNode for GeneratorCritic {
    fn conversation<'a>(facts: Self::Facts<'a>) -> &'a Conversation {
        facts.conversation
    }
}

impl GeneratorCriticNode for GeneratorCritic {
    fn count<'a>(facts: Self::Facts<'a>) -> SeatCount {
        facts.count
    }
}
