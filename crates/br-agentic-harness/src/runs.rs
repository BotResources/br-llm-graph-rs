use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use br_agentic_contract::InstanceId;
use br_agentic_harness_contract::{BoxId, ProcessId, RunEnd};

pub(crate) enum Closing {
    Open(Run),
    Opening,
    Nothing,
}

pub(crate) enum Committed {
    Kept,
    ClosedMeanwhile(Run, RunEnd),
}

pub(crate) struct Reservation<'a> {
    runs: &'a Runs,
    instance: InstanceId,
    settled: bool,
}

impl Reservation<'_> {
    pub(crate) fn commit(mut self, boxed: BoxId) -> Committed {
        self.settled = true;
        let mut state = self.runs.state();
        let closed = state.opening.remove(&self.instance).flatten();
        let run = Run {
            boxed,
            background: Vec::new(),
        };
        if let Some(end) = closed {
            return Committed::ClosedMeanwhile(run, end);
        }
        state.open.insert(self.instance, run);
        Committed::Kept
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if !self.settled {
            self.runs.state().opening.remove(&self.instance);
        }
    }
}

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
    opening: HashMap<InstanceId, Option<RunEnd>>,
}

impl Runs {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn reserve(&self, instance: InstanceId) -> Option<Reservation<'_>> {
        let mut state = self.state();
        if state.open.contains_key(&instance) || state.opening.contains_key(&instance) {
            return None;
        }
        state.opening.insert(instance, None);
        Some(Reservation {
            runs: self,
            instance,
            settled: false,
        })
    }

    pub(crate) fn close(&self, instance: InstanceId, end: RunEnd) -> Closing {
        let mut state = self.state();
        if let Some(run) = state.open.remove(&instance) {
            return Closing::Open(run);
        }
        match state.opening.get_mut(&instance) {
            Some(closed) => {
                closed.get_or_insert(end);
                Closing::Opening
            }
            None => Closing::Nothing,
        }
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
