use std::num::NonZeroU32;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepLimit(NonZeroU32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoundLimit(NonZeroU32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgrammaticRefusalLimit(NonZeroU32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidLimit {
    #[error("a step limit is at least 1")]
    Steps,
    #[error("a round limit is at least 1")]
    Rounds,
    #[error("a programmatic-refusal limit is at least 1")]
    Refusals,
    #[error("a timeout is longer than zero")]
    Timeout,
}

impl StepLimit {
    pub fn new(limit: u32) -> Result<Self, InvalidLimit> {
        NonZeroU32::new(limit).map(Self).ok_or(InvalidLimit::Steps)
    }

    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl RoundLimit {
    pub fn new(limit: u32) -> Result<Self, InvalidLimit> {
        NonZeroU32::new(limit).map(Self).ok_or(InvalidLimit::Rounds)
    }

    pub fn get(self) -> u32 {
        self.0.get()
    }
}

impl ProgrammaticRefusalLimit {
    pub fn new(limit: u32) -> Result<Self, InvalidLimit> {
        NonZeroU32::new(limit)
            .map(Self)
            .ok_or(InvalidLimit::Refusals)
    }

    pub fn get(self) -> u32 {
        self.0.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    model_call: Duration,
    tool_call: Duration,
}

impl Timeouts {
    pub fn new(model_call: Duration, tool_call: Duration) -> Result<Self, InvalidLimit> {
        if model_call.is_zero() || tool_call.is_zero() {
            return Err(InvalidLimit::Timeout);
        }
        Ok(Self {
            model_call,
            tool_call,
        })
    }

    pub fn model_call(&self) -> Duration {
        self.model_call
    }

    pub fn tool_call(&self) -> Duration {
        self.tool_call
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Steps {
    made: u32,
    limit: StepLimit,
}

impl Steps {
    pub(crate) fn new(limit: StepLimit) -> Self {
        Self { made: 0, limit }
    }

    pub(crate) fn count(&mut self) {
        self.made = self.made.saturating_add(1);
    }

    pub(crate) fn made(&self) -> u32 {
        self.made
    }

    pub(crate) fn remaining(&self) -> u32 {
        self.limit.get().saturating_sub(self.made)
    }
}
