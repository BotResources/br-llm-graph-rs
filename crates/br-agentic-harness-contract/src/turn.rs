use br_agentic_contract::ThreadRef;
use br_llm_messages::{AgentId, TurnId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct TurnRef {
    pub thread: ThreadRef,
    pub agent: AgentId,
    pub turn: TurnId,
    pub parent: Option<TurnId>,
}

impl TurnRef {
    pub fn new(thread: ThreadRef, agent: AgentId, turn: TurnId, parent: Option<TurnId>) -> Self {
        Self {
            thread,
            agent,
            turn,
            parent,
        }
    }
}
