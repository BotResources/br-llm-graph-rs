use std::fmt;

use br_graph_contract::{Cause, Fields, State};

use crate::Checkpoint;

pub enum Outcome<F: Fields> {
    RunEnded {
        state: State<F>,
    },
    RunFailed {
        failures: Vec<RunFailure>,
        checkpoint: Checkpoint,
    },
    RunCancelled {
        failures: Vec<RunFailure>,
        checkpoint: Checkpoint,
    },
}

impl<F: Fields> fmt::Debug for Outcome<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RunEnded { state } => f.debug_struct("RunEnded").field("state", state).finish(),
            Self::RunFailed {
                failures,
                checkpoint,
            } => f
                .debug_struct("RunFailed")
                .field("failures", failures)
                .field("checkpoint", checkpoint)
                .finish(),
            Self::RunCancelled {
                failures,
                checkpoint,
            } => f
                .debug_struct("RunCancelled")
                .field("failures", failures)
                .field("checkpoint", checkpoint)
                .finish(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RunFailure {
    #[error("node {node:?} failed: {reason}")]
    NodeFailed {
        node: String,
        reason: String,
        cause: Cause,
    },
    #[error("node {node:?} refused its input: {reason}")]
    NodeRefusedInput { node: String, reason: String },
    #[error("node {node:?} could not start: {reason}")]
    NodeCouldNotStart {
        node: String,
        reason: String,
        cause: Option<Cause>,
    },
    #[error("node {node:?} vanished: {reason}")]
    NodeVanished { node: String, reason: String },
    #[error("update of node {node:?} refused: {reason}")]
    UpdateRefused { node: String, reason: String },
    #[error("edge of node {node:?} failed: {reason}")]
    EdgeFailed {
        node: String,
        reason: String,
        cause: Option<Cause>,
    },
    #[error("checkpoint sink refused: {reason}")]
    SinkRefused {
        reason: String,
        checkpoint: Checkpoint,
    },
    #[error("checkpoint not built: {reason}")]
    CheckpointNotBuilt { reason: String },
}
