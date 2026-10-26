use std::fmt;

#[derive(Debug)]
pub struct ProbeCause;

impl fmt::Display for ProbeCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the probe cause")
    }
}

impl std::error::Error for ProbeCause {}

#[derive(Clone, Copy)]
pub enum CreateMode {
    Normal,
    FailCreate,
    WrongId,
    CloseInput,
    Hang,
}

#[derive(Clone, Copy)]
pub enum Mode {
    Answer,
    Gate,
    Silent,
    Fail,
    Refuse,
    CancelAlone,
    Vanish,
    Reject,
    Invalidate,
    Record,
    IgnoreCancel,
    SlowCancel,
    TriggerThenAnswer,
}
