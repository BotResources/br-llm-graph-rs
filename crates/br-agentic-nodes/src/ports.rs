use br_agentic_contract::{AgentLabel, PortFault};
use br_graph_contract::Fields;
use br_llm_clients_core::BoxFuture;
use br_llm_messages::{HumanId, ModelResponse, PlainText};

pub trait PromptPort<F: Fields>: Send + Sync {
    fn prompt<'a>(
        &'a self,
        snapshot: &'a F,
        label: &'a AgentLabel,
    ) -> BoxFuture<'a, Result<PlainText, PortFault>>;
}

pub trait OutcomePort<F: Fields>: Send + Sync {
    fn update<'a>(
        &'a self,
        snapshot: &'a F,
        outcome: &'a Outcome,
    ) -> BoxFuture<'a, Result<F::Update, PortFault>>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    response: ModelResponse,
    step_limit_reached: bool,
    verdict: Option<bool>,
}

impl Outcome {
    pub(crate) fn new(
        response: ModelResponse,
        step_limit_reached: bool,
        verdict: Option<bool>,
    ) -> Self {
        Self {
            response,
            step_limit_reached,
            verdict,
        }
    }

    pub fn response(&self) -> &ModelResponse {
        &self.response
    }

    pub fn step_limit_reached(&self) -> bool {
        self.step_limit_reached
    }

    pub fn verdict(&self) -> Option<bool> {
        self.verdict
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CriticRequest {
    pub text: PlainText,
    pub human: HumanId,
}

pub trait CriticRequestPort<F: Fields>: Send + Sync {
    fn request<'a>(&'a self, snapshot: &'a F) -> BoxFuture<'a, Result<CriticRequest, PortFault>>;
}
