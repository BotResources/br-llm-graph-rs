use std::collections::HashSet;

use br_agentic_contract::IncomingItem;
use br_llm_messages::{EntryId, IncomingEntry};

use crate::boundary::store_at_boundary;
use crate::fault::Fault;
use crate::ids::turn_id;
use crate::mail_entry::MailEntries;
use crate::worker::Worker;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Drained {
    pub stored: usize,
    pub request_stored: bool,
}

fn is_request(item: &IncomingItem) -> bool {
    match item {
        IncomingItem::Mail(_) => true,
        IncomingItem::Entry(entry) => matches!(entry.entry(), IncomingEntry::HumanMessage(_)),
    }
}

impl Worker<'_> {
    pub async fn drain_at_start(&mut self) -> Result<Drained, Fault> {
        self.read_incoming_now().await;
        self.drain().await
    }

    pub async fn drain(&mut self) -> Result<Drained, Fault> {
        let Some(incoming) = self.incoming.clone() else {
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
            .map(IncomingItem::id)
            .collect();
        let entries = MailEntries::new(self.known.clone());
        let boundary = store_at_boundary(
            &mut self.session,
            &*incoming,
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
