use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, PoisonError};

use br_agentic_contract::{
    Cursor, IncomingBatch, IncomingContext, IncomingPort, PortError, PortFuture,
};
use br_agentic_harness_contract::{HarnessHistory, MailPort, StoredAnswer};
use br_llm_messages::EntryId;
use tokio::runtime::Handle;
use tokio::task::JoinSet;

pub struct IncomingAdapter {
    history: Arc<dyn HarnessHistory>,
    mail: Arc<dyn MailPort>,
    waiting: Mutex<JoinSet<()>>,
}

impl IncomingAdapter {
    pub(crate) fn new(history: Arc<dyn HarnessHistory>, mail: Arc<dyn MailPort>) -> Self {
        Self {
            history,
            mail,
            waiting: Mutex::new(JoinSet::new()),
        }
    }
}

async fn mark_when_stored(
    history: Arc<dyn HarnessHistory>,
    mail: Arc<dyn MailPort>,
    context: IncomingContext,
    item: EntryId,
) {
    if let Ok(StoredAnswer::Stored) = history.stored(&context.thread, item).await {
        let _ = mail.mark_read(&context, item).await;
    }
}

impl IncomingPort for IncomingAdapter {
    fn items<'a>(
        &'a self,
        context: &'a IncomingContext,
        after: Option<&'a Cursor>,
        limit: NonZeroUsize,
    ) -> PortFuture<'a, Result<IncomingBatch, PortError>> {
        self.mail.items(context, after, limit)
    }

    fn mark_read<'a>(
        &'a self,
        context: &'a IncomingContext,
        item: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async move {
            let handle = Handle::try_current().map_err(|_| {
                PortError::fault("the harness needs a Tokio runtime to wait for the stored signal")
            })?;
            let mut waiting = self.waiting.lock().unwrap_or_else(PoisonError::into_inner);
            while waiting.try_join_next().is_some() {}
            waiting.spawn_on(
                mark_when_stored(
                    Arc::clone(&self.history),
                    Arc::clone(&self.mail),
                    *context,
                    item,
                ),
                &handle,
            );
            Ok(())
        })
    }
}
