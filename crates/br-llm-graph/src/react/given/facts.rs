//! What the context of each model-calling node holds beyond the state and
//! the configuration. Its types are public for the traits' sake (the
//! `Facts` of `ModelNode`), never reachable by name.

use br_llm_messages::{Conversation, Step};

/// What an LLM call's context holds beyond the state and the
/// configuration: the attempt.
#[derive(Clone, Copy)]
pub struct CallFacts {
    pub(crate) attempt: usize,
}

/// What a ReAct agent's context adds: its conversation and the round;
/// and, for its schedule, the 0-based index of its turn in the call; and
/// the attempt of the model call.
#[derive(Clone, Copy)]
pub struct AgentFacts<'a> {
    pub(crate) conversation: &'a Conversation,
    pub(crate) round: usize,
    pub(crate) turn: usize,
    pub(crate) attempt: usize,
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

/// What the generator's context adds: its conversation, the round, where
/// the call stands and the attempt of the model call.
#[derive(Clone, Copy)]
pub struct GeneratorFacts<'a> {
    pub(crate) conversation: &'a Conversation,
    pub(crate) round: usize,
    pub(crate) count: SeatCount,
    pub(crate) attempt: usize,
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
/// the final step judged. Its functions judge a generation, never a
/// reply asked for again: their attempt is 1.
#[derive(Clone, Copy)]
pub struct CheckFacts<'a> {
    pub(crate) conversation: &'a Conversation,
    pub(crate) count: SeatCount,
    pub(crate) answer: &'a Step,
}
