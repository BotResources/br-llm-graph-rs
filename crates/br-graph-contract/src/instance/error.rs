#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the input end of the instance is closed")]
pub struct InputClosed;
