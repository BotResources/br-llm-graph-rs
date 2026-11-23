use std::num::NonZeroUsize;

use br_agentic_contract::{IncomingContext, IncomingItem, IncomingPort, PortError};

#[derive(Debug, Default)]
pub struct Queue {
    items: Vec<IncomingItem>,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, item: IncomingItem) -> bool {
        if self.items.iter().any(|held| held.id() == item.id()) {
            return false;
        }
        self.items.push(item);
        true
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn items(&self) -> &[IncomingItem] {
        &self.items
    }

    pub async fn read(
        &mut self,
        incoming: &dyn IncomingPort,
        context: &IncomingContext,
        batch: NonZeroUsize,
    ) -> Result<usize, PortError> {
        let mut added = 0;
        let mut after = None;
        loop {
            let page = incoming.items(context, after.as_ref(), batch).await?;
            added += page
                .items
                .into_iter()
                .filter(|item| self.push(item.clone()))
                .count();
            match page.next {
                Some(cursor) if after.as_ref() == Some(&cursor) => {
                    return Err(PortError::fault(
                        "the incoming port gave the same cursor twice, the list does not advance",
                    ));
                }
                Some(cursor) => after = Some(cursor),
                None => return Ok(added),
            }
        }
    }

    pub(crate) fn take(&mut self) -> Vec<IncomingItem> {
        std::mem::take(&mut self.items)
    }

    pub(crate) fn restore(&mut self, items: Vec<IncomingItem>) {
        self.items.splice(0..0, items);
    }
}
