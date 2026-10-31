use br_graph_contract::{Cause, Reason};

#[derive(Debug, thiserror::Error)]
pub enum PortError {
    #[error("the request was refused: {0}")]
    Refused(Reason),
    #[error("a port failed")]
    Fault(#[source] Cause),
}

impl PortError {
    pub fn refused(reason: impl Into<String>) -> Self {
        Self::Refused(Reason::new(reason))
    }

    pub fn fault(cause: impl Into<Cause>) -> Self {
        Self::Fault(cause.into())
    }

    pub fn is_refusal(&self) -> bool {
        matches!(self, Self::Refused(_))
    }

    pub fn reason(&self) -> Option<&Reason> {
        match self {
            Self::Refused(reason) => Some(reason),
            Self::Fault(_) => None,
        }
    }
}
