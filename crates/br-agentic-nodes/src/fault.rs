use std::error::Error;
use std::time::Duration;

use br_agentic_contract::{BoundaryError, DuplicateTool, PortFault, ThreadError, ToolFault};
use br_llm_clients_core::ClientError;

#[derive(Debug, thiserror::Error)]
pub enum CallFailure {
    #[error("the client failed: {0}")]
    Client(#[from] ClientError),
    #[error("the call exceeded its timeout of {0:?}")]
    Timeout(Duration),
    #[error(
        "the client reported that the model should have thought, and the response holds no thinking block"
    )]
    NoThinking,
    #[error("the answer of the critic is unusable: {0}")]
    UnusableAnswer(String),
}

#[derive(Debug, thiserror::Error)]
pub enum Fault {
    #[error(transparent)]
    Port(#[from] PortFault),
    #[error(transparent)]
    Thread(#[from] ThreadError),
    #[error("the queue could not be stored: {0}")]
    Boundary(#[from] BoundaryError),
    #[error(transparent)]
    Tool(#[from] ToolFault),
    #[error(transparent)]
    DuplicateTool(#[from] DuplicateTool),
    #[error(transparent)]
    Model(#[from] CallFailure),
    #[error("no author name is known for the agent {label}")]
    NoAuthor { label: String },
    #[error("a value could not be built: {0}")]
    Value(String),
    #[error("the workspace port gave a path outside the sandbox: {0}")]
    Sandbox(String),
    #[error("the critic did not validate at the step limit: {message}")]
    CriticAtStepLimit { message: String },
    #[error("a panic: {0}")]
    Panic(String),
}

impl Fault {
    pub(crate) fn value(error: impl std::fmt::Display) -> Self {
        Fault::Value(error.to_string())
    }

    pub fn detail(&self) -> String {
        let mut parts = vec![self.to_string()];
        let mut source = self.source();
        while let Some(cause) = source {
            let text = cause.to_string();
            if parts.last() != Some(&text) {
                parts.push(text);
            }
            source = cause.source();
        }
        parts.join(": ")
    }
}

#[derive(Debug)]
pub enum Stop {
    Cancelled,
    Fault(Fault),
}

impl<E: Into<Fault>> From<E> for Stop {
    fn from(error: E) -> Self {
        Stop::Fault(error.into())
    }
}

pub(crate) enum AttemptError {
    Failed(CallFailure),
    Fatal(Fault),
}

impl From<CallFailure> for AttemptError {
    fn from(failure: CallFailure) -> Self {
        AttemptError::Failed(failure)
    }
}

macro_rules! fatal_from {
    ($($error:ty),+) => {
        $(impl From<$error> for AttemptError {
            fn from(error: $error) -> Self {
                AttemptError::Fatal(error.into())
            }
        })+
    };
}

fatal_from!(PortFault, ThreadError, Fault);
