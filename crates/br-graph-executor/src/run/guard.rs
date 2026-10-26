use tokio::task::{AbortHandle, JoinHandle};

pub(super) struct AbortGuard(AbortHandle);

impl AbortGuard {
    pub(super) fn new(handle: AbortHandle) -> Self {
        Self(handle)
    }
}

impl Drop for AbortGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(super) struct AbortOnDrop<T>(pub(super) JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}
