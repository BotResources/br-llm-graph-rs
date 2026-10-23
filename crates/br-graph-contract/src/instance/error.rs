#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the input end of the instance is closed")]
pub struct InputClosed;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EventEndError {
    #[error(
        "the event end has ended: the terminal event was already delivered or the instance was dropped"
    )]
    Finished,
    #[error("the node side vanished without sending a terminal event")]
    Vanished,
}
