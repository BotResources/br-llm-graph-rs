use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_agentic_contract::memory::MemoryIncoming;
use br_agentic_contract::{
    Cursor, IncomingBatch, IncomingContext, IncomingPort, PortError, PortFuture,
};
use br_llm_messages::EntryId;
use tokio::sync::Notify;

pub struct Guard(Arc<AtomicUsize>);

impl Drop for Guard {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

pub struct ProbeIncoming {
    inner: Arc<MemoryIncoming>,
    hang_from: usize,
    reads: AtomicUsize,
    pub started: Arc<Notify>,
    pub dropped: Arc<AtomicUsize>,
}

impl ProbeIncoming {
    pub fn hanging_from(inner: Arc<MemoryIncoming>, hang_from: usize) -> Arc<Self> {
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

impl IncomingPort for ProbeIncoming {
    fn items<'a>(
        &'a self,
        context: &'a IncomingContext,
        after: Option<&'a Cursor>,
        limit: NonZeroUsize,
    ) -> PortFuture<'a, Result<IncomingBatch, PortError>> {
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

    fn mark_read<'a>(
        &'a self,
        context: &'a IncomingContext,
        item: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.mark_read(context, item)
    }
}
