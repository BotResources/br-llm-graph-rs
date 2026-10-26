use std::fmt;

pub const CREATE_PANIC: &str = "creation exploded";

pub struct Poison;

impl Drop for Poison {
    fn drop(&mut self) {
        panic!("the payload dropped");
    }
}

#[derive(Debug)]
pub struct ProbeCause;

impl fmt::Display for ProbeCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("boom")
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
    FailWhenReleased,
    AnswerAndClose,
    GateThenNormal,
    PanicNow,
    PanicInFuture,
    PanicOpaque,
    PanicStatic,
    PanicPoison,
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
    TriggerOnApply,
    PanicOnApply,
    PanicWork,
}
