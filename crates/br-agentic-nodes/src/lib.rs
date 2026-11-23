#![doc = include_str!("../README.md")]

mod boundary;
mod client;
mod config;
mod drain;
mod ending;
mod fault;
mod generator_critic;
mod ids;
mod limits;
mod mail_entry;
mod pass;
mod queue;
mod react;
mod read;
mod run;
mod seats;
mod session;
mod session_error;
mod signals;
mod sink;
mod slot;
mod start;
mod step;
mod structure;
mod thinking;
mod tool_round;
mod warning;
mod worker;

pub use boundary::{Boundary, BoundaryError, MailEntryBuilder, store_at_boundary};
pub use client::{EmptyList, FallbackList, Ladder, ModelClient, ThinkingExpectation};
pub use config::{AuthorSource, Ports, Settings};
pub use drain::Drained;
pub use ending::{cancelled, failed, result};
pub use fault::{CallFailure, Fault, Stop};
pub use generator_critic::{
    CRITIC_LABEL, CriticRequestSource, GENERATOR_CRITIC, GENERATOR_LABEL, GeneratorCritic,
    GeneratorCriticSpec, PROGRAMMATIC_CRITIC_LABEL, ProgrammaticCritic, RoundLimitEnding,
    StepLimitEnding,
};
pub use limits::{InvalidLimit, ProgrammaticRefusalLimit, RoundLimit, StepLimit, Timeouts};
pub use queue::Queue;
pub use react::{AGENT_LABEL, REACT_AGENT, ReactAgent, ReactSpec};
pub use session::Session;
pub use session_error::SessionError;
pub use slot::Slot;
pub use structure::check_structure;
pub use thinking::{check_thinking, holds_thinking};
pub use warning::{WARNING_WINDOW, warning_text};
pub use worker::{Worker, WorkerParts};
