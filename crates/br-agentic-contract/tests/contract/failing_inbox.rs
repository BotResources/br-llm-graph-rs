use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_agentic_contract::memory::MemoryInbox;
use br_agentic_contract::{
    Cursor, InboxBatch, InboxContext, InboxPort, MailDocument, MailId, NewMail, PortError,
    PortFuture,
};
use br_llm_messages::EntryId;

pub enum FailingMarks {
    None,
    All,
    Only(Vec<EntryId>),
}

pub struct FailingInbox<'a> {
    inner: &'a MemoryInbox,
    pub marks: FailingMarks,
    pub items_before_failing: Option<usize>,
    pub fail_wait: bool,
    pub fail_send: bool,
    pub fail_documents: bool,
    items_calls: AtomicUsize,
    mark_calls: AtomicUsize,
}

impl<'a> FailingInbox<'a> {
    pub fn new(inner: &'a MemoryInbox) -> Self {
        Self {
            inner,
            marks: FailingMarks::None,
            items_before_failing: None,
            fail_wait: false,
            fail_send: false,
            fail_documents: false,
            items_calls: AtomicUsize::new(0),
            mark_calls: AtomicUsize::new(0),
        }
    }

    pub fn mark_calls(&self) -> usize {
        self.mark_calls.load(Ordering::SeqCst)
    }

    fn mark_fails(&self, item: EntryId) -> bool {
        match &self.marks {
            FailingMarks::None => false,
            FailingMarks::All => true,
            FailingMarks::Only(ids) => ids.contains(&item),
        }
    }
}

impl InboxPort for FailingInbox<'_> {
    fn items<'a>(
        &'a self,
        context: &'a InboxContext,
        after: Option<&'a Cursor>,
        limit: NonZeroUsize,
    ) -> PortFuture<'a, Result<InboxBatch, PortError>> {
        let call = self.items_calls.fetch_add(1, Ordering::SeqCst);
        match self.items_before_failing {
            Some(allowed) if call >= allowed => {
                Box::pin(async { Err(PortError::fault("the inbox read failed")) })
            }
            _ => self.inner.items(context, after, limit),
        }
    }

    fn wait<'a>(&'a self, context: &'a InboxContext) -> PortFuture<'a, Result<(), PortError>> {
        if self.fail_wait {
            return Box::pin(async { Err(PortError::fault("the inbox wait failed")) });
        }
        self.inner.wait(context)
    }

    fn mark_read<'a>(
        &'a self,
        context: &'a InboxContext,
        item: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.mark_calls.fetch_add(1, Ordering::SeqCst);
        if self.mark_fails(item) {
            return Box::pin(async { Err(PortError::fault("the inbox mark failed")) });
        }
        self.inner.mark_read(context, item)
    }

    fn send<'a>(
        &'a self,
        context: &'a InboxContext,
        mail: NewMail,
    ) -> PortFuture<'a, Result<(), PortError>> {
        if self.fail_send {
            return Box::pin(async { Err(PortError::fault("the inbox send failed")) });
        }
        self.inner.send(context, mail)
    }

    fn documents<'a>(
        &'a self,
        context: &'a InboxContext,
        mail: MailId,
    ) -> PortFuture<'a, Result<Vec<MailDocument>, PortError>> {
        if self.fail_documents {
            return Box::pin(async { Err(PortError::fault("the inbox documents failed")) });
        }
        self.inner.documents(context, mail)
    }
}
