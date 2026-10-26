use br_graph_contract::Cause;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MapReduceError {
    #[error("the initial value of internal state {position} is invalid: {reason}")]
    InvalidInitialValue { position: usize, reason: String },
    #[error("the internal instance {position} could not be created")]
    InstanceNotCreated {
        position: usize,
        #[source]
        cause: Cause,
    },
    #[error(
        "the internal instance {position} carries the instance id {returned}, not the id {expected} it was given"
    )]
    InstanceIdMismatch {
        position: usize,
        expected: String,
        returned: String,
    },
    #[error("the internal instance {position} refused its input: its input end is closed")]
    InputRefused { position: usize },
    #[error("the internal instance {position} failed")]
    InternalFailed {
        position: usize,
        #[source]
        cause: Cause,
    },
    #[error("the internal instance {position} refused its input: {reason}")]
    InternalRefused { position: usize, reason: String },
    #[error(
        "the internal instance {position} was cancelled while its cancel token was not triggered"
    )]
    InternalCancelled { position: usize },
    #[error("the internal instance {position} vanished without sending a terminal event")]
    InternalVanished { position: usize },
    #[error("the update of the internal instance {position} was not accepted: {reason}")]
    UpdateRejected { position: usize, reason: String },
}
