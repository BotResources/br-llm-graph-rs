//! The call a model-call step of the tests serves when it runs outside an
//! agent's graph: an agent's call in memory on `chat`, and the key where it
//! stands.

use crate::react::agent_call::AgentCall;
use crate::react::test_support::key;
use crate::state::Value;

/// The call of a model-call step run on `chat` outside an agent's graph,
/// in memory: its trigger is the last user input of `chat`, and where it
/// stands among its turns is the int key `turn_rank` (`TURN_RANK`, from 1),
/// which every schema such a step runs on declares.
pub(crate) fn in_memory_call() -> AgentCall {
    AgentCall {
        opening: key("chat"),
        position: key(TURN_RANK),
        failed: key("failed_turns"),
        thread: None,
        turns: None,
    }
}

/// The key where the call of `in_memory_call` stands.
pub(crate) const TURN_RANK: &str = "turn_rank";

/// The start value of `TURN_RANK`: the call writes its first turn.
pub(crate) fn first_turn() -> Value {
    Value::int(1)
}
