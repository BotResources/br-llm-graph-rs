use std::sync::Arc;

use br_agentic_contract::memory::MemoryThreadAccess;
use br_llm_messages::{
    AITurn, AgentId, ClosureReason, Conversation, ModelResponse, ResponseEnding, ResponseMetadata,
    Turn, TurnClosure, TurnEntry, TurnId,
};

use super::client::{Part, blocks};
use crate::common::{Rig, author, entry_id, turn_id};

pub fn seed_turn(
    conversation: &mut Conversation,
    agent: AgentId,
    parts: Vec<Part>,
    ending: ResponseEnding,
    closure: Option<ClosureReason>,
) -> TurnId {
    let id = turn_id();
    conversation
        .push_turn(Turn::from(AITurn::open(id, agent, author("Agent"))))
        .unwrap();
    let response =
        ModelResponse::new(entry_id(), blocks(&parts), ending, ResponseMetadata::new()).unwrap();
    conversation
        .push_entry(&id, TurnEntry::from(response))
        .unwrap();
    if let Some(reason) = closure {
        conversation
            .push_entry(&id, TurnEntry::from(TurnClosure::new(entry_id(), reason)))
            .unwrap();
    }
    id
}

pub fn text_part(text: &str) -> Vec<Part> {
    vec![Part::Text(text.to_owned())]
}

pub fn seeded(build: impl FnOnce(&Rig, &mut Conversation)) -> Rig {
    let mut rig = Rig::new();
    let mut conversation = rig.conversation();
    build(&rig, &mut conversation);
    rig.access = Arc::new(MemoryThreadAccess::new(
        &rig.record,
        rig.thread,
        conversation,
    ));
    rig
}
