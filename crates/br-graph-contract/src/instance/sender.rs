use std::fmt;
use std::sync::Arc;

use super::Event;
use super::core::Core;
use super::dropped::{Dropped, Watch};
use crate::{Fields, InstanceId};

pub struct EventSender<F: Fields> {
    pub(super) core: Arc<Core<F>>,
}

impl<F: Fields> EventSender<F> {
    pub(super) fn new(core: Arc<Core<F>>) -> Self {
        Self { core }
    }

    pub fn id(&self) -> InstanceId {
        self.core.id()
    }

    pub fn finish(self, event: Event<F>) {
        self.core.finish(event);
    }

    pub fn is_dropped(&self) -> bool {
        self.core.is_dropped()
    }

    pub fn dropped(&self) -> Dropped {
        Dropped::new(Arc::clone(&self.core) as Arc<dyn Watch>)
    }
}

impl<F: Fields> Drop for EventSender<F> {
    fn drop(&mut self) {
        self.core.sender_gone();
    }
}

impl<F: Fields> fmt::Debug for EventSender<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventSender")
            .field("id", &self.id())
            .finish()
    }
}
