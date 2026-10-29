use br_graph_contract::Reason;
use br_llm_messages::{EntryId, IncomingEntry};

#[derive(Debug, Clone, PartialEq)]
pub struct InboxEntry(IncomingEntry);

impl InboxEntry {
    pub fn new(entry: IncomingEntry) -> Result<Self, Reason> {
        match entry {
            IncomingEntry::HumanMessage(_)
            | IncomingEntry::RequestAccepted(_)
            | IncomingEntry::QueuePosition(_)
            | IncomingEntry::TaskStarted(_)
            | IncomingEntry::TaskCompleted(_)
            | IncomingEntry::TaskFailed(_)
            | IncomingEntry::RuntimeNotice(_) => Ok(Self(entry)),
            _ => Err(Reason::new(
                "an inbox entry is a human message, a runtime notification or a runtime notice",
            )),
        }
    }

    pub fn id(&self) -> EntryId {
        self.0.id()
    }

    pub fn entry(&self) -> &IncomingEntry {
        &self.0
    }

    pub fn into_entry(self) -> IncomingEntry {
        self.0
    }
}
