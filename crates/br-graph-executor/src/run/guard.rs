use tokio::task::AbortHandle;

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
