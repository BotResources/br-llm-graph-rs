use br_agentic_contract::PortError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OpenFault {
    #[error("no box was ready in the time that the provider gives to open")]
    ImageUnavailable,
    #[error("the boxes of the provider do not start")]
    ImageFailing,
}

impl OpenFault {
    pub fn of(error: &PortError) -> Option<Self> {
        match error {
            PortError::Fault(cause) => cause.downcast_ref::<Self>().copied(),
            PortError::Refused(_) => None,
        }
    }
}

impl From<OpenFault> for PortError {
    fn from(fault: OpenFault) -> Self {
        PortError::fault(fault)
    }
}
