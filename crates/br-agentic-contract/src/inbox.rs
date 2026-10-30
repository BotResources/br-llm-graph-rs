use std::num::NonZeroUsize;

use br_graph_contract::InstanceId;
use br_llm_messages::{AgentId, EntryId};

use crate::error::PortError;
use crate::future::PortFuture;
use crate::ids::{MailId, ThreadRef};
use crate::inbox_entry::InboxEntry;
use crate::mail::{MailDocument, MailItem, NewMail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InboxContext {
    pub instance: InstanceId,
    pub thread: ThreadRef,
    pub agent: AgentId,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InboxItem {
    Mail(MailItem),
    Entry(InboxEntry),
}

impl InboxItem {
    pub fn id(&self) -> EntryId {
        match self {
            InboxItem::Mail(item) => item.id().entry_id(),
            InboxItem::Entry(entry) => entry.id(),
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
pub struct InboxBatch {
    pub items: Vec<InboxItem>,
    pub next: Option<Cursor>,
}

pub trait InboxPort: Send + Sync {
    fn items<'a>(
        &'a self,
        context: &'a InboxContext,
        after: Option<&'a Cursor>,
        limit: NonZeroUsize,
    ) -> PortFuture<'a, Result<InboxBatch, PortError>>;

    fn wait<'a>(&'a self, context: &'a InboxContext) -> PortFuture<'a, Result<(), PortError>>;

    fn mark_read<'a>(
        &'a self,
        context: &'a InboxContext,
        item: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>>;

    fn send<'a>(
        &'a self,
        context: &'a InboxContext,
        mail: NewMail,
    ) -> PortFuture<'a, Result<(), PortError>>;

    fn documents<'a>(
        &'a self,
        context: &'a InboxContext,
        mail: MailId,
    ) -> PortFuture<'a, Result<Vec<MailDocument>, PortError>>;
}
