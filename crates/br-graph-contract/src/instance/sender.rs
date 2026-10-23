use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use super::Event;
use super::core::Core;
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

    pub fn close_input(&self) {
        self.core.close_input();
    }

    pub fn is_dropped(&self) -> bool {
        self.core.is_dropped()
    }

    pub fn dropped(&self) -> impl Future<Output = ()> + Send + 'static {
        Dropped {
            core: Arc::clone(&self.core),
            key: None,
        }
    }
}

struct Dropped<F: Fields> {
    core: Arc<Core<F>>,
    key: Option<u64>,
}

impl<F: Fields> Future for Dropped<F> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if this.core.poll_dropped(&mut this.key, cx.waker()) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

impl<F: Fields> Drop for Dropped<F> {
    fn drop(&mut self) {
        if let Some(key) = self.key.take() {
            self.core.remove_dropped_waiter(key);
        }
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
