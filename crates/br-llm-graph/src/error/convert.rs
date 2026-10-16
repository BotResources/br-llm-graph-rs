//! The conversions into `GraphError` of the dependencies' own errors.
//!
//! A thread store's error has none: `ThreadError` is any boxed error, so a
//! conversion would turn the error of a node, a tool or a hook into
//! `Thread` as well. The library wraps a store's error itself.
//!
//! ```
//! # use std::sync::Arc;
//! # use br_llm_graph::*;
//! let failed: ThreadError = "the store is down".into();
//! let _ = GraphError::Thread { source: Arc::from(failed) };
//! ```
//!
//! ```compile_fail,E0277
//! # use br_llm_graph::*;
//! let failed: ThreadError = "the store is down".into();
//! let _ = GraphError::from(failed);
//! ```

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
