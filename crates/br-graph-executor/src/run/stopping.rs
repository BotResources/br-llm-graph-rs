use br_graph_contract::Fields;
use tokio::time::Instant;

use super::{Phase, Run};
use crate::RunFailure;

impl<F: Fields> Run<F> {
    pub(super) fn is_stopping(&self) -> bool {
        matches!(self.phase, Phase::Stopping { .. })
    }

    pub(super) fn read_token(&mut self) {
        if !self.is_stopping() && self.cancel.handle().is_triggered() {
            self.begin_stopping();
        }
    }

    pub(super) fn begin_stopping(&mut self) {
        if !self.is_stopping() {
            self.phase = Phase::Stopping {
                deadline: Instant::now() + self.grace,
            };
        }
    }

    pub(super) fn fail(&mut self, failure: RunFailure) {
        self.failures.push(failure);
        if !self.is_stopping() {
            let handle = self.cancel.handle();
            if !handle.is_triggered() {
                handle.trigger();
                self.executor_triggered = true;
            }
            self.begin_stopping();
        }
    }

    pub(super) fn abandon_all_live(&mut self) {
        for slot in self.slots.values_mut() {
            if slot.is_live() {
                slot.abandon();
            }
        }
    }
}
