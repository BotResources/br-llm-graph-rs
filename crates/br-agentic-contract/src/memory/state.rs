use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use br_llm_messages::{AgentId, Author, Conversation, ConversationId, EntryId, ResponseDelta};
use tokio::sync::broadcast;

use crate::fault::{PortFault, ThreadError};
use crate::ids::{MailId, ThreadRef};
use crate::inbox_entry::InboxEntry;
use crate::mail::{Attachment, DocumentRef};

pub(super) struct StoredMail {
    pub seq: u64,
    pub id: MailId,
    pub sender: AgentId,
    pub to: Vec<AgentId>,
    pub cc: Vec<AgentId>,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<Attachment>,
    pub read: HashSet<AgentId>,
}

pub(super) struct StoredEntry {
    pub seq: u64,
    pub agent: AgentId,
    pub entry: InboxEntry,
}

pub(super) struct OpenObject {
    pub alive: Arc<AtomicBool>,
}

#[derive(Default)]
pub(super) struct State {
    pub threads: HashMap<ThreadRef, Conversation>,
    pub authors: HashMap<AgentId, Author>,
    pub sources: HashMap<AgentId, ConversationId>,
    pub mails: Vec<StoredMail>,
    pub entries: Vec<StoredEntry>,
    pub documents: HashMap<DocumentRef, Vec<u8>>,
    pub open: HashMap<EntryId, OpenObject>,
    pub deltas: HashMap<EntryId, Arc<Mutex<Vec<ResponseDelta>>>>,
    pub fragments: HashMap<EntryId, Arc<Mutex<Vec<String>>>>,
    pub next_seq: u64,
    pub given: HashMap<AgentId, u64>,
}

impl State {
    pub fn take_seq(&mut self) -> u64 {
        self.next_seq += 1;
        self.next_seq
    }

    pub fn conversation(&self, thread: &ThreadRef) -> Result<&Conversation, ThreadError> {
        self.threads.get(thread).ok_or_else(|| unknown(thread))
    }

    pub fn conversation_mut(
        &mut self,
        thread: &ThreadRef,
    ) -> Result<&mut Conversation, ThreadError> {
        self.threads.get_mut(thread).ok_or_else(|| unknown(thread))
    }
}

fn unknown(thread: &ThreadRef) -> ThreadError {
    ThreadError::Fault(PortFault::new(format!("unknown thread {thread}")))
}

#[derive(Clone)]
pub struct MemoryHub {
    pub(super) state: Arc<Mutex<State>>,
    pub(super) arrivals: broadcast::Sender<AgentId>,
}

impl Default for MemoryHub {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryHub {
    pub fn new() -> Self {
        let (arrivals, _) = broadcast::channel(1024);
        Self {
            state: Arc::new(Mutex::new(State::default())),
            arrivals,
        }
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn announce(&self, agent: AgentId) {
        let _ = self.arrivals.send(agent);
    }
}
