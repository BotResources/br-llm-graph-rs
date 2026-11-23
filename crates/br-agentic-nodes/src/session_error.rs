use br_agentic_contract::{ApplyError, MailId, PortError};
use br_llm_messages::{Error, TurnId};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SessionError {
    #[error("the rules of br-llm-messages refuse the change: {0}")]
    Rules(#[source] Error),
    #[error("the turn {0} is not the AI turn of that agent and author")]
    WrongTurn(TurnId),
    #[error("the entry built for the mail {0} is not the entry of that mail")]
    WrongMailEntry(MailId),
    #[error(transparent)]
    Port(PortError),
}

impl From<Error> for SessionError {
    fn from(error: Error) -> Self {
        Self::Rules(error)
    }
}

impl From<ApplyError> for SessionError {
    fn from(error: ApplyError) -> Self {
        match error {
            ApplyError::Rules(error) => Self::Rules(error),
            ApplyError::WrongTurn(turn) => Self::WrongTurn(turn),
            other => Self::Port(PortError::fault(other.to_string())),
        }
    }
}
