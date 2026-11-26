use std::time::SystemTime;

use br_agentic_contract::PortError;
use br_llm_messages::TurnId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SaveReceipt {
    pub turn: TurnId,
    pub made_at: SystemTime,
    pub bytes: u64,
}

impl SaveReceipt {
    pub fn new(turn: TurnId, made_at: SystemTime, bytes: u64) -> Self {
        Self {
            turn,
            made_at,
            bytes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlreadySaved;

impl AlreadySaved {
    pub const REASON: &'static str =
        "a save of this thread, this agent and this turn already exists";

    pub fn refusal() -> PortError {
        PortError::refused(Self::REASON)
    }

    pub fn is(error: &PortError) -> bool {
        error
            .reason()
            .is_some_and(|reason| reason.as_str() == Self::REASON)
    }
}
