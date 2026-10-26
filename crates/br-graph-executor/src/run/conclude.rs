use br_graph_contract::Fields;

use super::Run;
use crate::{Checkpoint, Observation, Outcome, OutcomeKind};

impl<F: Fields> Run<F> {
    pub(super) fn conclude(mut self, starting: Option<Checkpoint>) -> Outcome<F> {
        if self.all_idle() {
            self.observing
                .tell(|| Observation::Outcome(OutcomeKind::RunEnded));
            return Outcome::RunEnded {
                state: (*self.state).clone(),
            };
        }
        let checkpoint = starting.unwrap_or_else(|| self.outcome_checkpoint());
        let failures = std::mem::take(&mut self.failures);
        if self.executor_triggered {
            self.observing
                .tell(|| Observation::Outcome(OutcomeKind::RunFailed));
            Outcome::RunFailed {
                failures,
                checkpoint,
            }
        } else {
            self.observing
                .tell(|| Observation::Outcome(OutcomeKind::RunCancelled));
            Outcome::RunCancelled {
                failures,
                checkpoint,
            }
        }
    }
}
