use br_llm_messages::{EntryId, IncomingEntry};

#[derive(Debug, Clone, PartialEq)]
pub struct InboxEntry(IncomingEntry);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("an inbox entry is a human message, a runtime notification or a runtime notice")]
pub struct InvalidInboxEntry;

impl InboxEntry {
    pub fn new(entry: IncomingEntry) -> Result<Self, InvalidInboxEntry> {
        match entry {
            IncomingEntry::HumanMessage(_)
            | IncomingEntry::RequestAccepted(_)
            | IncomingEntry::QueuePosition(_)
            | IncomingEntry::TaskStarted(_)
            | IncomingEntry::TaskCompleted(_)
            | IncomingEntry::TaskFailed(_)
            | IncomingEntry::RuntimeNotice(_) => Ok(Self(entry)),
            _ => Err(InvalidInboxEntry),
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
