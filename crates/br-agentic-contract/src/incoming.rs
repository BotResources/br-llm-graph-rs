use std::num::NonZeroUsize;

use br_graph_contract::InstanceId;
use br_llm_messages::{AgentId, EntryId};

use crate::entry_item::EntryItem;
use crate::error::PortError;
use crate::future::PortFuture;
use crate::ids::ThreadRef;
use crate::mail::MailItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IncomingContext {
    pub instance: InstanceId,
    pub thread: ThreadRef,
    pub agent: AgentId,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IncomingItem {
    Mail(MailItem),
    Entry(EntryItem),
}

impl IncomingItem {
    pub fn id(&self) -> EntryId {
        match self {
            IncomingItem::Mail(item) => item.id().entry_id(),
            IncomingItem::Entry(entry) => entry.id(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor(String);

impl Cursor {
    pub fn new(position: impl Into<String>) -> Self {
        Self(position.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IncomingBatch {
    pub items: Vec<IncomingItem>,
    pub next: Option<Cursor>,
}

pub trait IncomingPort: Send + Sync {
    fn items<'a>(
        &'a self,
        context: &'a IncomingContext,
        after: Option<&'a Cursor>,
        limit: NonZeroUsize,
    ) -> PortFuture<'a, Result<IncomingBatch, PortError>>;

    fn mark_read<'a>(
        &'a self,
        context: &'a IncomingContext,
        item: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>>;
}
