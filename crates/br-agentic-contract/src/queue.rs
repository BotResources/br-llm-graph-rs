use std::num::NonZeroUsize;

use crate::error::PortError;
use crate::inbox::{InboxContext, InboxItem, InboxPort};

#[derive(Debug, Default)]
pub struct Queue {
    items: Vec<InboxItem>,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, item: InboxItem) -> bool {
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

    pub fn items(&self) -> &[InboxItem] {
        &self.items
    }

    pub async fn read(
        &mut self,
        inbox: &dyn InboxPort,
        context: &InboxContext,
        batch: NonZeroUsize,
    ) -> Result<usize, PortError> {
        let mut added = 0;
        let mut after = None;
        loop {
            let page = inbox.items(context, after.as_ref(), batch).await?;
            added += page
                .items
                .into_iter()
                .filter(|item| self.push(item.clone()))
                .count();
            match page.next {
                Some(cursor) => after = Some(cursor),
                None => return Ok(added),
            }
        }
    }

    pub(crate) fn take(&mut self) -> Vec<InboxItem> {
        std::mem::take(&mut self.items)
    }

    pub(crate) fn restore(&mut self, items: Vec<InboxItem>) {
        self.items.splice(0..0, items);
    }
}
