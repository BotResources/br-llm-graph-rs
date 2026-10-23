use std::num::NonZeroUsize;

use crate::Reason;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid state: {reason}")]
pub struct InvalidState {
    reason: Reason,
}

impl InvalidState {
    pub(crate) fn new(reason: impl Into<Reason>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    pub fn reason(&self) -> Reason {
        self.reason.clone()
    }

    pub(super) fn oversize(size: usize, limit: NonZeroUsize) -> Self {
        Self::new(format!(
            "serialized fields take {size} bytes, over the limit of {limit} bytes"
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApplyError {
    #[error("update refused by the fields: {0}")]
    Refused(Reason),
    #[error(transparent)]
    InvalidState(#[from] InvalidState),
}
