use br_agentic_contract::{AgentLabel, PortError, PortFuture};
use br_graph_contract::Fields;
use br_llm_messages::{HumanId, ModelResponse};

use crate::outcome::{GeneratorCriticOutcome, ReactOutcome};

pub trait PromptPort<F: Fields>: Send + Sync {
    fn prompt<'a>(
        &'a self,
        snapshot: &'a F,
        label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<String, PortError>>;
}

pub trait AuthorPort<F: Fields>: Send + Sync {
    fn author<'a>(
        &'a self,
        snapshot: &'a F,
        label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<String, PortError>>;
}

pub trait ReactOutcomePort<F: Fields>: Send + Sync {
    fn update<'a>(
        &'a self,
        snapshot: &'a F,
        outcome: &'a ReactOutcome,
    ) -> PortFuture<'a, Result<F::Update, PortError>>;
}

pub trait GeneratorCriticOutcomePort<F: Fields>: Send + Sync {
    fn update<'a>(
        &'a self,
        snapshot: &'a F,
        outcome: &'a GeneratorCriticOutcome,
    ) -> PortFuture<'a, Result<F::Update, PortError>>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct CriticRequest {
    pub text: String,
    pub human: HumanId,
}

pub trait CriticRequestPort<F: Fields>: Send + Sync {
    fn request<'a>(
        &'a self,
        snapshot: &'a F,
        response: &'a ModelResponse,
    ) -> PortFuture<'a, Result<CriticRequest, PortError>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgrammaticVerdict {
    Accept,
    Refuse(String),
}

pub trait ProgrammaticCriticPort<F: Fields>: Send + Sync {
    fn check<'a>(
        &'a self,
        snapshot: &'a F,
        response: &'a ModelResponse,
        refusals_in_a_row: u32,
    ) -> PortFuture<'a, Result<ProgrammaticVerdict, PortError>>;
}
