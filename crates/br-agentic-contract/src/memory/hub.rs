use std::sync::PoisonError;

use br_llm_messages::{
    AgentId, Author, Conversation, ConversationId, EntryId, ResponseDelta, Uuid,
};

use super::state::{MemoryHub, StoredEntry};
use crate::ids::{MailId, ThreadRef};
use crate::inbox_entry::InboxEntry;
use crate::mail::DocumentRef;

impl MemoryHub {
    pub fn create_thread(&self) -> ThreadRef {
        let thread = ThreadRef::new(Uuid::now_v7());
        let conversation = ConversationId::from_uuid(thread.as_uuid());
        if let Ok(conversation) = conversation {
            self.lock()
                .threads
                .insert(thread, Conversation::new(conversation));
        }
        thread
    }

    pub fn conversation(&self, thread: &ThreadRef) -> Option<Conversation> {
        self.lock().threads.get(thread).cloned()
    }

    pub fn set_author(&self, agent: AgentId, author: Author) {
        self.lock().authors.insert(agent, author);
    }

    pub fn set_source(&self, agent: AgentId, conversation: ConversationId) {
        self.lock().sources.insert(agent, conversation);
    }

    pub fn put_document(&self, reference: DocumentRef, content: Vec<u8>) {
        self.lock().documents.insert(reference, content);
    }

    pub fn deliver_entry(&self, agent: AgentId, entry: InboxEntry) {
        {
            let mut state = self.lock();
            let seq = state.take_seq();
            state.entries.push(StoredEntry { seq, agent, entry });
        }
        self.announce(agent);
    }

    pub fn is_read(&self, agent: AgentId, mail: MailId) -> bool {
        self.lock()
            .mails
            .iter()
            .find(|stored| stored.id == mail)
            .is_some_and(|stored| stored.read.contains(&agent))
    }

    pub fn is_open(&self, entry: EntryId) -> bool {
        self.lock().open.contains_key(&entry)
    }

    pub fn response_deltas(&self, entry: EntryId) -> Vec<ResponseDelta> {
        self.lock()
            .deltas
            .get(&entry)
            .map(|log| log.lock().unwrap_or_else(PoisonError::into_inner).clone())
            .unwrap_or_default()
    }

    pub fn result_fragments(&self, entry: EntryId) -> Vec<String> {
        self.lock()
            .fragments
            .get(&entry)
            .map(|log| log.lock().unwrap_or_else(PoisonError::into_inner).clone())
            .unwrap_or_default()
    }
}
