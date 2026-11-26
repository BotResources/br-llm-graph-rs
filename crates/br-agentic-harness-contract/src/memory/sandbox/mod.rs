mod arrange;
mod claim;
mod hold;
mod looks;
mod port;
mod state;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use br_agentic_contract::WebAccess;

use super::record::HarnessRecord;
use state::State;

pub use hold::OpenHold;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Network {
    Closed,
    Open(WebAccess),
}

#[derive(Clone)]
pub struct MemorySandbox {
    record: HarnessRecord,
    state: Arc<Mutex<State>>,
}

impl MemorySandbox {
    pub fn new(record: &HarnessRecord) -> Self {
        Self {
            record: record.clone(),
            state: Arc::default(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
