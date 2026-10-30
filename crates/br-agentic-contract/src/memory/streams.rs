use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use br_llm_clients_core::StreamSink;
use br_llm_messages::{EntryId, ResponseDelta};

use super::record::{Call, Record};
use crate::future::PortFuture;
use crate::stream::ResultStream;

pub(super) struct MemoryResponseStream {
    pub entry: EntryId,
    pub alive: Arc<AtomicBool>,
    pub record: Record,
}

impl StreamSink for MemoryResponseStream {
    fn send<'a>(&'a mut self, delta: ResponseDelta) -> PortFuture<'a, ()> {
        if self.alive.load(Ordering::SeqCst) {
            self.record.push(Call::ResponseDelta {
                entry: self.entry,
                delta,
            });
        }
        Box::pin(async {})
    }
}

pub(super) struct MemoryResultStream {
    pub entry: EntryId,
    pub alive: Arc<AtomicBool>,
    pub record: Record,
}

impl ResultStream for MemoryResultStream {
    fn send<'a>(&'a mut self, fragment: String) -> PortFuture<'a, ()> {
        if self.alive.load(Ordering::SeqCst) {
            self.record.push(Call::ResultFragment {
                entry: self.entry,
                fragment,
            });
        }
        Box::pin(async {})
    }
}
