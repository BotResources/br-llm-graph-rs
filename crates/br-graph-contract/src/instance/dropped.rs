use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use super::core::Core;
use crate::Fields;

pub(super) trait Watch: Send + Sync {
    fn poll_dropped(&self, key: &mut Option<u64>, waker: &Waker) -> bool;

    fn remove_waiter(&self, key: u64);
}

impl<F: Fields> Watch for Core<F> {
    fn poll_dropped(&self, key: &mut Option<u64>, waker: &Waker) -> bool {
        Core::poll_dropped(self, key, waker)
    }

    fn remove_waiter(&self, key: u64) {
        self.remove_dropped_waiter(key);
    }
}

pub struct Dropped {
    watch: Arc<dyn Watch>,
    key: Option<u64>,
}

impl Dropped {
    pub(super) fn new(watch: Arc<dyn Watch>) -> Self {
        Self { watch, key: None }
    }
}

impl Future for Dropped {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if this.watch.poll_dropped(&mut this.key, cx.waker()) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

impl Drop for Dropped {
    fn drop(&mut self) {
        if let Some(key) = self.key.take() {
            self.watch.remove_waiter(key);
        }
    }
}

impl fmt::Debug for Dropped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dropped").finish_non_exhaustive()
    }
}
