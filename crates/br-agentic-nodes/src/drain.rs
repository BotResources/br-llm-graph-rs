use std::collections::HashSet;

use br_agentic_contract::{InboxItem, store_at_boundary};
use br_llm_messages::{EntryId, IncomingEntry};

use crate::fault::Fault;
use crate::ids::turn_id;
use crate::mail_entry::MailEntries;
use crate::worker::Worker;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Drained {
    pub stored: usize,
    pub request_stored: bool,
}

fn is_request(item: &InboxItem) -> bool {
    match item {
        InboxItem::Mail(_) => true,
        InboxItem::Entry(entry) => matches!(entry.entry(), IncomingEntry::HumanMessage(_)),
    }
}

impl Worker<'_> {
    pub async fn drain(&mut self) -> Result<Drained, Fault> {
        let Some(inbox) = self.inbox.clone() else {
            return Ok(Drained::default());
        };
        self.refresh_read().await;
        if self.queue.is_empty() {
            return Ok(Drained::default());
        }
        let requests: HashSet<EntryId> = self
            .queue
            .items()
            .iter()
            .filter(|item| is_request(item))
            .map(InboxItem::id)
            .collect();
        let entries = MailEntries::new(self.known.clone());
        let boundary = store_at_boundary(
            &mut self.session,
            &*inbox,
            &self.context,
            &mut self.queue,
            &|item| entries.build(item),
            turn_id()?,
        )
        .await?;
        let request_stored = boundary.stored.iter().any(|id| requests.contains(id));
        if request_stored {
            self.counters.reset();
        }
        Ok(Drained {
            stored: boundary.stored.len(),
            request_stored,
        })
    }
}
