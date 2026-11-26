use std::time::SystemTime;

use br_llm_messages::TurnId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FreshReason {
    NoSaveEver,
    SaveExpired { expired_at: Option<SystemTime> },
    SaveFailed { turn: TurnId },
    RestoreFailed { turn: TurnId },
    StoreUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartKind {
    Continued,
    Restored { turn: TurnId },
    Fresh(FreshReason),
}
