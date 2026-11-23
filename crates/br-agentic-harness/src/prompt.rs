use std::sync::Arc;

use br_agentic_contract::{AgentLabel, Fields, PortError, PortFuture, PromptPort};

pub struct PromptAdapter<F: Fields> {
    inner: Arc<dyn PromptPort<F>>,
}

impl<F: Fields> PromptAdapter<F> {
    pub(crate) fn new(inner: Arc<dyn PromptPort<F>>) -> Self {
        Self { inner }
    }
}

impl<F: Fields> PromptPort<F> for PromptAdapter<F> {
    fn prompt<'a>(
        &'a self,
        snapshot: &'a F,
        label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<String, PortError>> {
        self.inner.prompt(snapshot, label)
    }
}
