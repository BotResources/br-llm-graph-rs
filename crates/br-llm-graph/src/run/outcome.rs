use crate::error::GraphError;
use crate::run::checkpoint::Checkpoint;
use crate::state::State;
use crate::value::EndLabel;

/// How a run ended. A failure is not an outcome: it is a `RunFailure`.
pub enum Outcome {
    /// No node was left to run.
    Finished {
        /// The final state.
        state: State,
        /// The end label of the final superstep.
        end: EndLabel,
    },
    /// A pause was honoured at the end of a superstep that did not end the
    /// run.
    Paused {
        /// The state after that superstep and the next cursor.
        checkpoint: Checkpoint,
    },
    /// A cancel stopped a superstep, which was not applied.
    Cancelled {
        /// The state from before the superstep plus the inputs held so far,
        /// the cursor of that superstep and the run's pending entries.
        checkpoint: Checkpoint,
    },
}

/// A failed run: its error, and the checkpoint to resume it from.
#[derive(Debug)]
pub struct RunFailure {
    /// The state from before the failed superstep plus the inputs held
    /// meanwhile (when they apply), the cursor of the whole superstep and
    /// the run's pending entries.
    pub checkpoint: Checkpoint,
    /// The error.
    pub error: GraphError,
}

impl std::fmt::Display for RunFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "run failed: {}", self.error)
    }
}

impl std::error::Error for RunFailure {}
