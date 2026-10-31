#![doc = include_str!("../README.md")]

mod client;
mod config;
mod drain;
mod fault;
mod generator_critic;
mod ids;
mod limits;
mod mail_entry;
mod mail_tools;
mod outcome;
mod pass;
mod ports;
mod react;
mod read;
mod run;
mod sandbox;
mod seats;
mod signals;
mod sink;
mod slot;
mod start;
mod step;
mod thinking;
mod tool_round;
mod warning;
mod worker;

pub use client::{EmptyList, FallbackList, Ladder, ModelClient, ThinkingExpectation};
pub use config::{AuthorSource, Ports, Settings};
pub use drain::Drained;
pub use fault::{CallFailure, Fault, Stop};
pub use generator_critic::{
    CRITIC_LABEL, CriticRequestSource, GENERATOR_CRITIC, GENERATOR_LABEL, GeneratorCritic,
    GeneratorCriticSpec, PROGRAMMATIC_CRITIC_LABEL, ProgrammaticCritic, RoundLimitEnding,
    StepLimitEnding,
};
pub use limits::{InvalidLimit, ProgrammaticRefusalLimit, RoundLimit, StepLimit, Timeouts};
pub use mail_tools::{GET_MAIL_FILES, SEND_MAIL, mail_tools};
pub use outcome::{GeneratorCriticOutcome, Judgement, ReactOutcome};
pub use ports::{
    AuthorPort, CriticRequest, CriticRequestPort, GeneratorCriticOutcomePort,
    ProgrammaticCriticPort, ProgrammaticVerdict, PromptPort, ReactOutcomePort,
};
pub use react::{AGENT_LABEL, REACT_AGENT, ReactAgent, ReactSpec};
pub use sandbox::{FileSystem, SandboxError, SandboxPath};
pub use slot::Slot;
pub use thinking::{check_thinking, holds_thinking};
pub use warning::{WARNING_WINDOW, warning_text};
pub use worker::{Worker, WorkerParts};
