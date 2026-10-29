use std::num::NonZeroUsize;

use br_agentic_contract::{Cursor, store_at_boundary};

use crate::fault::Fault;
use crate::ids::turn_id;
use crate::worker::Worker;

const BATCH: NonZeroUsize = match NonZeroUsize::new(64) {
    Some(batch) => batch,
    None => NonZeroUsize::MIN,
};

impl Worker<'_> {
    pub(crate) async fn read_inbox(&mut self) -> Result<(), Fault> {
        let Some(inbox) = self.inbox else {
            return Ok(());
        };
        let mut cursor: Option<Cursor> = None;
        loop {
            let batch = inbox.items(&self.context, cursor.as_ref(), BATCH).await?;
            for item in batch.items {
                self.queue.push(item);
            }
            match batch.next {
                Some(next) => cursor = Some(next),
                None => return Ok(()),
            }
        }
    }

    pub async fn drain(&mut self) -> Result<usize, Fault> {
        let Some(inbox) = self.inbox else {
            return Ok(0);
        };
        self.read_inbox().await?;
        if self.queue.is_empty() {
            return Ok(0);
        }
        let boundary = store_at_boundary(
            self.access,
            inbox,
            &self.context,
            &mut self.queue,
            turn_id()?,
        )
        .await?;
        Ok(boundary.stored.len())
    }
}
