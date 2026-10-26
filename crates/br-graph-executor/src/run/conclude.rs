use std::sync::Arc;

use br_graph_contract::Fields;

use super::Run;
use crate::{Checkpoint, Observation, Outcome, OutcomeKind};

impl<F: Fields> Run<F> {
    pub(super) fn conclude(mut self, starting: Option<Checkpoint>) -> Outcome<F> {
        if self.all_idle() && self.failures.is_empty() {
            self.observing
                .tell(|| Observation::Outcome(OutcomeKind::RunEnded));
            let state = Arc::try_unwrap(self.state).unwrap_or_else(|shared| (*shared).clone());
            return Outcome::RunEnded { state };
        }
        let checkpoint = match starting {
            Some(checkpoint) => checkpoint,
            None => {
                let (checkpoint, not_built) = self.outcome_checkpoint();
                self.failures.extend(not_built);
                checkpoint
            }
        };
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
