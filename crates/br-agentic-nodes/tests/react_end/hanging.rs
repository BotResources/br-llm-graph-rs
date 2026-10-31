use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_agentic_contract::memory::MemoryInbox;
use br_agentic_contract::{
    Cursor, InboxBatch, InboxContext, InboxPort, MailDocument, MailId, NewMail, PortError,
    PortFuture,
};
use br_llm_messages::EntryId;
use tokio::sync::Notify;

pub struct Guard(Arc<AtomicUsize>);

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

pub struct ProbeInbox {
    inner: Arc<MemoryInbox>,
    hang_from: usize,
    reads: AtomicUsize,
    pub started: Arc<Notify>,
    pub dropped: Arc<AtomicUsize>,
}

impl ProbeInbox {
    pub fn hanging_from(inner: Arc<MemoryInbox>, hang_from: usize) -> Arc<Self> {
        Arc::new(Self {
            inner,
            hang_from,
            reads: AtomicUsize::new(0),
            started: Arc::new(Notify::new()),
            dropped: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }

    pub fn dropped(&self) -> usize {
        self.dropped.load(Ordering::SeqCst)
    }
}

impl InboxPort for ProbeInbox {
    fn items<'a>(
        &'a self,
        context: &'a InboxContext,
        after: Option<&'a Cursor>,
        limit: NonZeroUsize,
    ) -> PortFuture<'a, Result<InboxBatch, PortError>> {
        let nth = self.reads.fetch_add(1, Ordering::SeqCst);
        if nth < self.hang_from {
            return self.inner.items(context, after, limit);
        }
        Box::pin(async move {
            let _guard = Guard(self.dropped.clone());
            self.started.notify_one();
            std::future::pending().await
        })
    }

    fn wait<'a>(&'a self, context: &'a InboxContext) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.wait(context)
    }

    fn mark_read<'a>(
        &'a self,
        context: &'a InboxContext,
        item: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.mark_read(context, item)
    }

    fn send<'a>(
        &'a self,
        context: &'a InboxContext,
        mail: NewMail,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.send(context, mail)
    }

    fn documents<'a>(
        &'a self,
        context: &'a InboxContext,
        mail: MailId,
    ) -> PortFuture<'a, Result<Vec<MailDocument>, PortError>> {
        self.inner.documents(context, mail)
    }
}
