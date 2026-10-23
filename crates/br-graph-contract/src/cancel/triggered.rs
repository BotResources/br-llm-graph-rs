use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use super::shared::Shared;

pub struct Triggered {
    shared: Arc<Shared>,
    key: Option<u64>,
}

impl Triggered {
    pub(super) fn new(shared: Arc<Shared>) -> Self {
        Self { shared, key: None }
    }
}

impl Future for Triggered {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if this.shared.poll_triggered(&mut this.key, cx.waker()) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

impl Drop for Triggered {
    fn drop(&mut self) {
        if let Some(key) = self.key.take() {
            self.shared.remove_waiter(key);
        }
    }
}

impl fmt::Debug for Triggered {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Triggered")
            .field("triggered", &self.shared.is_triggered())
            .finish()
    }
}
