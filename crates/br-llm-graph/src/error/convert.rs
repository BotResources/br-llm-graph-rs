use br_llm_clients_core::{BuildError, ClientError};
use br_llm_messages::MessageError;

use crate::error::GraphError;

impl From<MessageError> for GraphError {
    fn from(error: MessageError) -> Self {
        GraphError::Message(error)
    }
}

impl From<ClientError> for GraphError {
    fn from(error: ClientError) -> Self {
        GraphError::Model(error)
    }
}

impl From<BuildError> for GraphError {
    fn from(error: BuildError) -> Self {
        GraphError::Request(error)
    }
}

impl From<crate::react::ThreadError> for GraphError {
    /// The error of a thread store: `Thread`, carrying it.
    fn from(error: crate::react::ThreadError) -> Self {
        GraphError::Thread {
            source: std::sync::Arc::from(error),
        }
    }
}
