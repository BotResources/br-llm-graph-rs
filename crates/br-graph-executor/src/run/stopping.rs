use std::time::Duration;

use br_graph_contract::Fields;
use tokio::time::Instant;

use super::{Phase, Run, Trigger};
use crate::RunFailure;

impl<F: Fields> Run<F> {
    pub(super) fn is_stopping(&self) -> bool {
        matches!(self.phase, Phase::Stopping { .. })
    }

    pub(super) fn stopped_by_executor(&self) -> bool {
        matches!(
            self.phase,
            Phase::Stopping {
                trigger: Trigger::Executor,
                ..
            }
        )
    }

    pub(super) fn read_token(&mut self) {
        if !self.is_stopping() && self.cancel.handle().is_triggered() {
            self.begin_stopping(Trigger::Host);
        }
    }

    fn begin_stopping(&mut self, trigger: Trigger) {
        self.phase = Phase::Stopping {
            deadline: deadline_after(self.grace),
            trigger,
        };
    }

    pub(super) fn leave_untaken(&mut self, node: &str) {
        if let Some(slot) = self.slots.get_mut(node) {
            slot.leave_untaken();
        }
    }

    pub(super) fn fail_node(&mut self, node: &str, failure: RunFailure) {
        self.leave_untaken(node);
        self.fail(failure);
    }

    pub(super) fn fail(&mut self, failure: RunFailure) {
        self.failures.push(failure);
        if !self.is_stopping() {
            let handle = self.cancel.handle();
            let trigger = if handle.is_triggered() {
                Trigger::Host
            } else {
                handle.trigger();
                Trigger::Executor
            };
            self.begin_stopping(trigger);
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

const FAR_FUTURE: Duration = Duration::from_secs(60 * 60 * 24 * 365 * 30);

fn deadline_after(grace: Duration) -> Instant {
    Instant::now() + grace.min(FAR_FUTURE)
}
