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

    pub(super) fn begin_stopping(&mut self, trigger: Trigger) {
        if !self.is_stopping() {
            self.phase = Phase::Stopping {
                deadline: deadline_after(self.grace),
                trigger,
            };
        }
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
    let now = Instant::now();
    now.checked_add(grace)
        .or_else(|| now.checked_add(FAR_FUTURE))
        .unwrap_or(now)
}
