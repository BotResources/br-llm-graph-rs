use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_agentic_contract::memory::MemoryIncoming;
use br_agentic_contract::{
    Cursor, IncomingBatch, IncomingContext, IncomingPort, PortError, PortFuture,
};
use br_llm_messages::EntryId;

pub enum FailingMarks {
    None,
    All,
    Only(Vec<EntryId>),
}

pub struct FailingIncoming<'a> {
    inner: &'a MemoryIncoming,
    pub marks: FailingMarks,
    pub items_before_failing: Option<usize>,
    pub stuck_cursor: bool,
    items_calls: AtomicUsize,
    mark_calls: AtomicUsize,
}

impl<'a> FailingIncoming<'a> {
    pub fn new(inner: &'a MemoryIncoming) -> Self {
        Self {
            inner,
            marks: FailingMarks::None,
            items_before_failing: None,
            stuck_cursor: false,
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

impl IncomingPort for FailingIncoming<'_> {
    fn items<'a>(
        &'a self,
        context: &'a IncomingContext,
        after: Option<&'a Cursor>,
        limit: NonZeroUsize,
    ) -> PortFuture<'a, Result<IncomingBatch, PortError>> {
        let call = self.items_calls.fetch_add(1, Ordering::SeqCst);
        match self.items_before_failing {
            Some(allowed) if call >= allowed => {
                Box::pin(async { Err(PortError::fault("the incoming read failed")) })
            }
            _ if self.stuck_cursor => {
                let page = self.inner.items(context, None, limit);
                Box::pin(async move {
                    tokio::task::yield_now().await;
                    let mut batch = page.await?;
                    batch.next = Some(Cursor::new("stuck"));
                    Ok(batch)
                })
            }
            _ => self.inner.items(context, after, limit),
        }
    }

    fn mark_read<'a>(
        &'a self,
        context: &'a IncomingContext,
        item: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.mark_calls.fetch_add(1, Ordering::SeqCst);
        if self.mark_fails(item) {
            return Box::pin(async { Err(PortError::fault("the incoming mark failed")) });
        }
        self.inner.mark_read(context, item)
    }
}
