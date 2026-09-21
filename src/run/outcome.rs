use crate::error::GraphError;
use crate::run::checkpoint::Checkpoint;
use crate::state::State;
use crate::value::EndLabel;

pub enum Outcome {
    Finished { state: State, end: EndLabel },
    Paused { checkpoint: Checkpoint },
    Cancelled { checkpoint: Checkpoint },
}

#[derive(Debug)]
pub struct RunFailure {
    pub checkpoint: Checkpoint,
    pub error: GraphError,
}

impl std::fmt::Display for RunFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "run failed: {}", self.error)
    }
}

impl std::error::Error for RunFailure {}
