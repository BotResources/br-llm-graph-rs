use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use super::shared::Shared;

pub struct CancelToken {
    handle: CancelHandle,
    view: CancelView,
}

#[derive(Clone)]
pub struct CancelHandle {
    pub(super) shared: Arc<Shared>,
}

#[derive(Clone)]
pub struct CancelView {
    pub(super) shared: Arc<Shared>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::from_shared(Shared::new(false))
    }

    fn from_shared(shared: Arc<Shared>) -> Self {
        Self {
            handle: CancelHandle {
                shared: Arc::clone(&shared),
            },
            view: CancelView { shared },
        }
    }

    pub fn handle(&self) -> &CancelHandle {
        &self.handle
    }

    pub fn view(&self) -> CancelView {
        self.view.clone()
    }

    pub fn child(&self) -> CancelToken {
        self.view.child()
    }

    pub fn into_parts(self) -> (CancelHandle, CancelView) {
        (self.handle, self.view)
    }
}

impl Default for CancelToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancelHandle {
    pub fn trigger(&self) {
        self.shared.trigger();
    }

    pub fn is_triggered(&self) -> bool {
        self.shared.is_triggered()
    }

    pub fn view(&self) -> CancelView {
        CancelView {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl CancelView {
    pub fn is_triggered(&self) -> bool {
        self.shared.is_triggered()
    }

    pub fn triggered(&self) -> impl Future<Output = ()> + Send + 'static {
        Triggered {
            shared: Arc::clone(&self.shared),
            key: None,
        }
    }

    pub fn child(&self) -> CancelToken {
        CancelToken::from_shared(self.shared.child())
    }
}

struct Triggered {
    shared: Arc<Shared>,
    key: Option<u64>,
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

impl fmt::Debug for CancelToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CancelToken")
            .field("triggered", &self.view.is_triggered())
            .finish()
    }
}

impl fmt::Debug for CancelHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CancelHandle")
            .field("triggered", &self.is_triggered())
            .finish()
    }
}

impl fmt::Debug for CancelView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CancelView")
            .field("triggered", &self.is_triggered())
            .finish()
    }
}
