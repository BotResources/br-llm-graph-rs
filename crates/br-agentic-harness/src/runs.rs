use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use br_agentic_contract::InstanceId;
use br_agentic_harness_contract::{BoxId, ProcessId};

pub(crate) struct Run {
    pub(crate) boxed: BoxId,
    pub(crate) background: Vec<ProcessId>,
}

#[derive(Default)]
pub(crate) struct Runs {
    open: Mutex<HashMap<InstanceId, Run>>,
}

impl Runs {
    pub(crate) fn insert(&self, instance: InstanceId, boxed: BoxId) -> bool {
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        if open.contains_key(&instance) {
            return false;
        }
        open.insert(
            instance,
            Run {
                boxed,
                background: Vec::new(),
            },
        );
        true
    }

    pub(crate) fn take(&self, instance: InstanceId) -> Option<Run> {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&instance)
    }

    pub(crate) fn boxed(&self, instance: InstanceId) -> Option<BoxId> {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&instance)
            .map(|run| run.boxed.clone())
    }

    pub(crate) fn track(&self, instance: InstanceId, process: ProcessId) -> bool {
        match self
            .open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(&instance)
        {
            Some(run) => {
                run.background.push(process);
                true
            }
            None => false,
        }
    }
}
