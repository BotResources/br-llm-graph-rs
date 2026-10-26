use std::future::Future;
use std::pin::Pin;

use crate::Checkpoint;

pub type SinkFuture<'a> = Pin<Box<dyn Future<Output = Result<(), SinkRefusal>> + Send + 'a>>;

pub trait CheckpointSink: Send + Sync {
    fn accept<'a>(&'a self, checkpoint: &'a Checkpoint) -> SinkFuture<'a>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("checkpoint refused: {reason}")]
pub struct SinkRefusal {
    reason: String,
}

impl SinkRefusal {
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }
}
