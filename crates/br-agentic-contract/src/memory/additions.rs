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
    match conversation.turn(&target.id()) {
        Ok(turn) => {
            check_target(turn, agent, target)?;
            conversation
                .push_entry(&target.id(), entry)
                .map_err(refused)
        }
        Err(_) => append_new(conversation, agent, target, entry),
    }
}

pub(super) fn check_target(
    turn: &Turn,
    agent: AgentId,
    target: &TurnTarget,
) -> Result<(), ThreadError> {
    match (turn, target) {
        (Turn::AITurn(ai), TurnTarget::Ai { author, .. })
            if ai.agent() == agent && ai.author() == author =>
        {
            Ok(())
        }
        (Turn::AITurn(_), TurnTarget::Ai { .. }) => Err(not_theirs()),
        (Turn::AITurn(_), TurnTarget::Input(_)) => Err(not_theirs()),
        _ => Ok(()),
    }
}

pub(super) fn not_theirs() -> ThreadError {
    ThreadError::Refused(br_graph_contract::Reason::new(
        "the AI turn belongs to another agent or to another author",
    ))
}

fn append_new(
    conversation: &mut Conversation,
    agent: AgentId,
    target: &TurnTarget,
    entry: TurnEntry,
) -> Result<(), ThreadError> {
    match target {
        TurnTarget::Ai { id, author } => {
            let mut turn = AITurn::open(*id, agent, author.clone());
            turn.push_entry(entry).map_err(refused)?;
            conversation.push_turn(Turn::from(turn)).map_err(refused)
        }
        TurnTarget::Input(_) => Err(ThreadError::Refused(br_graph_contract::Reason::new(
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
        let fresh: Vec<&Incoming> = items
            .iter()
            .filter(|item| !holds_entry(conversation, item.id()))
            .collect();
        if conversation.turn(&turn).is_ok() || fresh.is_empty() {
            return Ok(Stored::Held);
        }
        let entries = fresh
            .into_iter()
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
        agent: AgentId,
        turn: TurnId,
        closure: TurnClosure,
    ) -> Result<Stored, ThreadError> {
        let mut state = self.lock();
        let conversation = state.conversation(thread)?;
        if holds_entry(conversation, closure.id()) {
            return Ok(Stored::Held);
        }
        if let Ok(Turn::AITurn(ai)) = conversation.turn(&turn)
            && ai.agent() != agent
        {
            return Err(not_theirs());
        }
        let mut next = conversation.clone();
        next.push_entry(&turn, TurnEntry::from(closure))
            .map_err(refused)?;
        *state.conversation_mut(thread)? = next;
        Ok(Stored::Added)
    }
}
