use std::sync::Arc;

use br_graph_contract::Fields;

use super::Run;
use crate::{Checkpoint, Observation, Outcome, OutcomeKind};

impl<F: Fields> Run<F> {
    pub(super) fn conclude(mut self, starting: Option<Checkpoint>) -> Outcome<F> {
        if self.all_idle() && !self.stopped_by_executor() {
            self.observing
                .tell(|| Observation::Outcome(OutcomeKind::RunEnded));
            let state = Arc::try_unwrap(self.state).unwrap_or_else(|shared| (*shared).clone());
            return Outcome::RunEnded { state };
        }
        let checkpoint = starting.unwrap_or_else(|| self.outcome_checkpoint());
        let failures = std::mem::take(&mut self.failures);
        if self.stopped_by_executor() {
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
