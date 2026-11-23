use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, PoisonError};

use br_agentic_contract::InstanceId;
use br_agentic_harness_contract::{BoxId, ProcessId};

pub(crate) struct Run {
    pub(crate) boxed: BoxId,
    pub(crate) background: Vec<ProcessId>,
}

#[derive(Default)]
pub(crate) struct Runs {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    open: HashMap<InstanceId, Run>,
    opening: HashSet<InstanceId>,
}

impl Runs {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn reserve(&self, instance: InstanceId) -> bool {
        let mut state = self.state();
        if state.open.contains_key(&instance) {
            return false;
        }
        state.opening.insert(instance)
    }

    pub(crate) fn cancel(&self, instance: InstanceId) {
        self.state().opening.remove(&instance);
    }

    pub(crate) fn commit(&self, instance: InstanceId, boxed: BoxId) {
        let mut state = self.state();
        state.opening.remove(&instance);
        state.open.insert(
            instance,
            Run {
                boxed,
                background: Vec::new(),
            },
        );
    }

    pub(crate) fn take(&self, instance: InstanceId) -> Option<Run> {
        self.state().open.remove(&instance)
    }

    pub(crate) fn boxed(&self, instance: InstanceId) -> Option<BoxId> {
        self.state()
            .open
            .get(&instance)
            .map(|run| run.boxed.clone())
    }

    pub(crate) fn track(&self, instance: InstanceId, process: ProcessId) -> bool {
        match self.state().open.get_mut(&instance) {
            Some(run) => {
                run.background.push(process);
                true
            }
            None => false,
        }
    }
}
