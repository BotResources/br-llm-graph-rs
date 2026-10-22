//! The nodes that call a model, sealed, and what the context of each holds
//! beyond the state and the configuration.

use br_llm_messages::Conversation;

use crate::react::generator_critic::GeneratorCritic;
use crate::react::llm_call::LlmCall;
use crate::react::react_agent::ReactAgent;
use crate::react::seats::{CriticSeat, GeneratorSeat};

pub(crate) use crate::react::given::facts::{
    AgentFacts, CallFacts, CheckFacts, CriticFacts, GeneratorFacts, SeatCount,
};

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

    #[doc(hidden)]
    fn attempt<'a>(facts: Self::Facts<'a>) -> usize;
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
        attempt: usize,
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
    type Facts<'a> = CallFacts;

    fn attempt<'a>(facts: Self::Facts<'a>) -> usize {
        facts.attempt
    }
}

impl ModelNode for ReactAgent {
    type Facts<'a> = AgentFacts<'a>;

    fn attempt<'a>(facts: Self::Facts<'a>) -> usize {
        facts.attempt
    }
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
        attempt: usize,
    ) -> Self::Facts<'a> {
        AgentFacts {
            conversation,
            round,
            turn,
            attempt,
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

    fn attempt<'a>(facts: Self::Facts<'a>) -> usize {
        facts.attempt
    }
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
        attempt: usize,
    ) -> Self::Facts<'a> {
        GeneratorFacts {
            conversation,
            round,
            count,
            attempt,
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
        // Saturating: a budget of `usize::MAX` allows as many calls.
        if final_generation {
            budget.saturating_add(1)
        } else {
            budget.max(1)
        }
    }
}

impl ModelNode for CriticSeat {
    type Facts<'a> = CriticFacts<'a>;

    fn attempt<'a>(facts: Self::Facts<'a>) -> usize {
        facts.attempt
    }
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

    /// Its functions judge a generation, never a reply asked for again.
    fn attempt<'a>(_facts: Self::Facts<'a>) -> usize {
        1
    }
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
