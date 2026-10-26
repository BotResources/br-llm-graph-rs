use std::any::Any;

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

pub(super) fn panic_reason(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        return (*text).to_owned();
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    "panic without a text message".to_owned()
}
