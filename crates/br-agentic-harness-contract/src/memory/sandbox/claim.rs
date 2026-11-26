use super::MemorySandbox;
use crate::sandbox::BoxId;

pub(super) struct Claim {
    sandbox: MemorySandbox,
    held: BoxId,
    armed: bool,
}

impl Claim {
    pub(super) fn new(sandbox: &MemorySandbox, held: BoxId) -> Self {
        Self {
            sandbox: sandbox.clone(),
            held,
            armed: true,
        }
    }

    pub(super) fn keep(mut self) {
        self.armed = false;
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if self.armed {
            self.sandbox.lock().release_box(&self.held);
        }
    }
}
