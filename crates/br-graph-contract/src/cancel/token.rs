use std::fmt;
use std::sync::Arc;

use super::shared::Shared;
use super::triggered::Triggered;

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
    #[must_use = "a token that is dropped at once is useless"]
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

    #[must_use = "a child token that is dropped at once is useless"]
    pub fn child(&self) -> CancelToken {
        self.view.child()
    }

    #[must_use = "dropping both parts discards the token"]
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

    pub fn triggered(&self) -> Triggered {
        Triggered::new(Arc::clone(&self.shared))
    }

    #[must_use = "a child token that is dropped at once is useless"]
    pub fn child(&self) -> CancelToken {
        CancelToken::from_shared(self.shared.child())
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
