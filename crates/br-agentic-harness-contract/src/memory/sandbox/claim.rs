use super::MemorySandbox;
use super::state::Kept;
use crate::sandbox::BoxId;

pub(super) struct Claim {
    sandbox: MemorySandbox,
    held: BoxId,
    kept: Option<Kept>,
    armed: bool,
}

impl Claim {
    pub(super) fn new(sandbox: &MemorySandbox, held: BoxId, kept: Option<Kept>) -> Self {
        Self {
            sandbox: sandbox.clone(),
            held,
            kept,
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
            let kept = self.kept.take();
            self.sandbox.lock().abandon_open(&self.held, kept);
        }
    }
}
