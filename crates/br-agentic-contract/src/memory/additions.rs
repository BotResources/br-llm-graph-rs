use br_llm_messages::{
    AITurn, AgentId, Conversation, EntryId, IncomingEntry, InputTurn, Turn, TurnClosure, TurnEntry,
    TurnId,
};

use super::build::{incoming_entry, refused};
use super::state::MemoryHub;
use crate::fault::ThreadError;
use crate::ids::ThreadRef;
use crate::thread_access::{Incoming, Stored, TurnTarget};

pub(super) fn holds_entry(conversation: &Conversation, id: EntryId) -> bool {
    conversation.entry(&id).is_ok()
}

pub(super) fn append(
    conversation: &mut Conversation,
    agent: AgentId,
    target: &TurnTarget,
    entry: TurnEntry,
) -> Result<(), ThreadError> {
    let exists = conversation.turn(&target.id()).is_ok();
    match (exists, target) {
        (true, _) => conversation
            .push_entry(&target.id(), entry)
            .map_err(refused),
        (false, TurnTarget::Ai { id, author }) => {
            let mut turn = AITurn::open(*id, agent, author.clone());
            turn.push_entry(entry).map_err(refused)?;
            conversation.push_turn(Turn::from(turn)).map_err(refused)
        }
        (false, TurnTarget::Input(_)) => Err(ThreadError::Refused(br_graph_contract::Reason::new(
            "the input turn does not exist",
        ))),
    }
}

impl MemoryHub {
    pub(super) fn add_input_turn(
        &self,
        thread: &ThreadRef,
        turn: TurnId,
        items: &[Incoming],
    ) -> Result<Stored, ThreadError> {
        let mut state = self.lock();
        let conversation = state.conversation(thread)?;
        let held = conversation.turn(&turn).is_ok()
            || items
                .iter()
                .any(|item| holds_entry(conversation, item.id()));
        if held {
            return Ok(Stored::Held);
        }
        let entries = items
            .iter()
            .map(|item| incoming_entry(&state, conversation, item))
            .collect::<Result<Vec<IncomingEntry>, _>>()?;
        let input = InputTurn::new(turn, entries).map_err(refused)?;
        let mut next = conversation.clone();
        next.push_turn(Turn::from(input)).map_err(refused)?;
        *state.conversation_mut(thread)? = next;
        Ok(Stored::Added)
    }

    pub(super) fn add_incoming(
        &self,
        thread: &ThreadRef,
        agent: AgentId,
        target: &TurnTarget,
        item: &Incoming,
    ) -> Result<Stored, ThreadError> {
        let mut state = self.lock();
        let conversation = state.conversation(thread)?;
        if holds_entry(conversation, item.id()) {
            return Ok(Stored::Held);
        }
        let entry =
            TurnEntry::try_from(incoming_entry(&state, conversation, item)?).map_err(refused)?;
        let mut next = conversation.clone();
        append(&mut next, agent, target, entry)?;
        *state.conversation_mut(thread)? = next;
        Ok(Stored::Added)
    }

    pub(super) fn close_turn(
        &self,
        thread: &ThreadRef,
        turn: TurnId,
        closure: TurnClosure,
    ) -> Result<Stored, ThreadError> {
        let mut state = self.lock();
        let conversation = state.conversation(thread)?;
        if holds_entry(conversation, closure.id()) {
            return Ok(Stored::Held);
        }
        let mut next = conversation.clone();
        next.push_entry(&turn, TurnEntry::from(closure))
            .map_err(refused)?;
        *state.conversation_mut(thread)? = next;
        Ok(Stored::Added)
    }
}
