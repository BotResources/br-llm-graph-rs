use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use br_graph_contract::CancelToken;

use crate::{CheckpointSink, Observer};

pub struct RunConfig {
    pub cancel: CancelToken,
    pub grace: Duration,
    pub sink: Option<Arc<dyn CheckpointSink>>,
    pub observer: Option<Arc<dyn Observer>>,
}

impl RunConfig {
    pub fn new(cancel: CancelToken, grace: Duration) -> Self {
        Self {
            cancel,
            grace,
            sink: None,
            observer: None,
        }
    }
}

impl fmt::Debug for RunConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RunConfig")
            .field("cancel", &self.cancel)
            .field("grace", &self.grace)
            .field("sink", &self.sink.is_some())
            .field("observer", &self.observer.is_some())
            .finish()
    }
}
