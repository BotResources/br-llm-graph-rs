use std::num::NonZeroUsize;
use std::sync::Arc;

use br_agentic_contract::{IncomingContext, IncomingItem, IncomingPort};
use tokio::task::JoinHandle;

use crate::queue::Queue;
use crate::worker::Worker;

const BATCH: NonZeroUsize = match NonZeroUsize::new(64) {
    Some(batch) => batch,
    None => NonZeroUsize::MIN,
};

pub(crate) struct RunningRead {
    handle: Option<JoinHandle<Vec<IncomingItem>>>,
}

impl RunningRead {
    fn spawn(incoming: Arc<dyn IncomingPort>, context: IncomingContext) -> Self {
        let handle = tokio::spawn(async move {
            let mut queue = Queue::new();
            let _ = queue.read(&*incoming, &context, BATCH).await;
            queue.items().to_vec()
        });
        Self {
            handle: Some(handle),
        }
    }

    fn is_finished(&self) -> bool {
        self.handle.as_ref().is_none_or(JoinHandle::is_finished)
    }

    async fn items(mut self) -> Vec<IncomingItem> {
        match self.handle.take() {
            Some(handle) => handle.await.unwrap_or_default(),
            None => Vec::new(),
        }
    }
}

impl Drop for RunningRead {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

impl Worker<'_> {
    pub(crate) async fn read_incoming_now(&mut self) {
        if let Some(incoming) = self.incoming.clone() {
            let _ = self.queue.read(&*incoming, &self.context, BATCH).await;
        }
    }

    pub(crate) async fn refresh_read(&mut self) {
        let Some(incoming) = self.incoming.clone() else {
            return;
        };
        match self.read.take() {
            Some(read) if read.is_finished() => {
                for item in read.items().await {
                    self.queue.push(item);
                }
            }
            Some(read) => {
                self.read = Some(read);
                return;
            }
            None => {}
        }
        self.read = Some(RunningRead::spawn(incoming, self.context));
    }

    pub(crate) fn stop_read(&mut self) {
        self.read = None;
    }
}
