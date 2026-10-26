use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

use crate::host::within;

#[derive(Clone, Default)]
pub struct Gate(Arc<Notify>);

impl Gate {
    pub fn release(&self) {
        self.0.notify_one();
    }

    pub(super) async fn passed(&self) {
        within(self.0.notified()).await;
    }
}

#[derive(Clone, Default)]
pub struct DropFlag(Arc<AtomicBool>);

impl DropFlag {
    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

pub(super) struct DropGuard(pub(super) DropFlag);

impl Drop for DropGuard {
    fn drop(&mut self) {
        self.0.0.store(true, Ordering::SeqCst);
    }
}
