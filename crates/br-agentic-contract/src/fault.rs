use br_graph_contract::{Cause, Reason};

#[derive(Debug, thiserror::Error)]
#[error("a port failed")]
pub struct PortFault {
    #[source]
    cause: Cause,
}

impl PortFault {
    pub fn new(cause: impl Into<Cause>) -> Self {
        Self {
            cause: cause.into(),
        }
    }

    pub fn cause(&self) -> &Cause {
        &self.cause
    }

    pub fn into_cause(self) -> Cause {
        self.cause
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ThreadError {
    #[error("the thread access refused the addition: {0}")]
    Refused(Reason),
    #[error(transparent)]
    Fault(#[from] PortFault),
}

#[derive(Debug, thiserror::Error)]
pub enum InboxError {
    #[error("the inbox refused the mail: {0}")]
    MailRefused(Reason),
    #[error(transparent)]
    Fault(#[from] PortFault),
}
