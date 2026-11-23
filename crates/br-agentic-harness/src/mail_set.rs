use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, PoisonError};

use br_agentic_contract::{InstanceId, ThreadRef};
use br_llm_messages::{Conversation, EntryId, EntryView, IncomingEntry, InputTurn, TurnEntry};

#[derive(Default)]
pub(crate) struct MailSet {
    held: Mutex<HashMap<ThreadRef, HashSet<EntryId>>>,
    bound: Mutex<HashMap<InstanceId, HashSet<ThreadRef>>>,
}

impl MailSet {
    pub(crate) fn record(&self, thread: ThreadRef, ids: impl IntoIterator<Item = EntryId>) {
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(thread)
            .or_default()
            .extend(ids);
    }

    pub(crate) fn bind(&self, instance: InstanceId, thread: ThreadRef) {
        self.bound
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(instance)
            .or_default()
            .insert(thread);
    }

    pub(crate) fn release(&self, instance: InstanceId) {
        let threads = self
            .bound
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&instance)
            .unwrap_or_default();
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        for thread in threads {
            held.remove(&thread);
        }
    }

    pub(crate) fn holds(&self, thread: &ThreadRef, id: EntryId) -> bool {
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(thread)
            .is_some_and(|ids| ids.contains(&id))
    }
}

fn incoming_mail(entry: &IncomingEntry) -> Option<EntryId> {
    match entry {
        IncomingEntry::AgentMail(mail) => Some(mail.id()),
        _ => None,
    }
}

pub(crate) fn turn_entry_mail(entry: &TurnEntry) -> Option<EntryId> {
    match entry {
        TurnEntry::AgentMail(mail) => Some(mail.id()),
        _ => None,
    }
}

pub(crate) fn input_turn_mails(turn: &InputTurn) -> Vec<EntryId> {
    turn.entries().iter().filter_map(incoming_mail).collect()
}

pub(crate) fn conversation_mails(conversation: &Conversation) -> Vec<EntryId> {
    conversation
        .entries()
        .filter_map(|placed| match placed.entry() {
            EntryView::Incoming(entry) => incoming_mail(entry),
            EntryView::Turn(entry) => turn_entry_mail(entry),
            _ => None,
        })
        .collect()
}
