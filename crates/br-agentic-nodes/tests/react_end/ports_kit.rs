use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use br_agentic_contract::{
    AgentLabel, AgentPort, PortError, PortFuture, RunContext, ThreadPort, Tool, ToolsPort,
};
use br_agentic_contract::{AuthorPort, ReactOutcome, ReactOutcomePort};
use br_graph_contract::InstanceId;
use br_llm_messages::AgentId;

use crate::common::{Doc, Recorded, text_of};

pub struct Trip {
    calls: AtomicUsize,
    hang: bool,
}

impl Trip {
    pub fn failing() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            hang: false,
        })
    }

    pub fn hanging() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            hang: true,
        })
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    async fn fire<T>(&self) -> Result<T, PortError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.hang {
            return std::future::pending().await;
        }
        Err(PortError::fault("the port failed"))
    }
}

pub struct FailingThreadPort(pub Arc<Trip>);

impl ThreadPort<Doc> for FailingThreadPort {
    fn thread<'a>(
        &'a self,
        _snapshot: &'a Doc,
    ) -> PortFuture<'a, Result<br_agentic_contract::ThreadRef, PortError>> {
        Box::pin(self.0.fire())
    }
}

pub struct FailingAgentPort(pub Arc<Trip>);

impl AgentPort<Doc> for FailingAgentPort {
    fn agent<'a>(
        &'a self,
        _snapshot: &'a Doc,
        _instance: InstanceId,
        _label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<AgentId, PortError>> {
        Box::pin(self.0.fire())
    }
}

pub struct FailingToolsPort(pub Arc<Trip>);

impl ToolsPort<Doc> for FailingToolsPort {
    fn tools<'a>(
        &'a self,
        _snapshot: &'a Doc,
        _context: &'a RunContext,
    ) -> PortFuture<'a, Result<Vec<Tool>, PortError>> {
        Box::pin(self.0.fire())
    }
}

pub struct FailingPrompt(pub Arc<Trip>);

impl br_agentic_contract::PromptPort<Doc> for FailingPrompt {
    fn prompt<'a>(
        &'a self,
        _snapshot: &'a Doc,
        _label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<String, PortError>> {
        Box::pin(self.0.fire())
    }
}

pub struct FailingAuthor(pub Arc<Trip>);

impl AuthorPort<Doc> for FailingAuthor {
    fn author<'a>(
        &'a self,
        _snapshot: &'a Doc,
        _label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<String, PortError>> {
        Box::pin(self.0.fire())
    }
}

pub struct GivenAuthor(pub String);

impl AuthorPort<Doc> for GivenAuthor {
    fn author<'a>(
        &'a self,
        _snapshot: &'a Doc,
        _label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<String, PortError>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

pub struct FailingOutcome(pub Arc<Trip>);

impl ReactOutcomePort<Doc> for FailingOutcome {
    fn update<'a>(
        &'a self,
        _snapshot: &'a Doc,
        _outcome: &'a ReactOutcome,
    ) -> PortFuture<'a, Result<Recorded, PortError>> {
        Box::pin(self.0.fire())
    }
}

pub struct SnapshotOutcome {
    pub notes: Mutex<Vec<String>>,
}

impl SnapshotOutcome {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            notes: Mutex::new(Vec::new()),
        })
    }

    pub fn notes(&self) -> Vec<String> {
        self.notes.lock().unwrap().clone()
    }
}

impl ReactOutcomePort<Doc> for SnapshotOutcome {
    fn update<'a>(
        &'a self,
        snapshot: &'a Doc,
        outcome: &'a ReactOutcome,
    ) -> PortFuture<'a, Result<Recorded, PortError>> {
        self.notes.lock().unwrap().push(snapshot.note.clone());
        Box::pin(async move {
            Ok(Recorded {
                text: text_of(outcome.response()),
                step_limit: outcome.step_limit_reached(),
                judgement: None,
                steps: Some(outcome.steps()),
                iterations: None,
            })
        })
    }
}

pub struct SnapshotPrompt {
    notes: Mutex<Vec<String>>,
}

impl SnapshotPrompt {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            notes: Mutex::new(Vec::new()),
        })
    }

    pub fn notes(&self) -> Vec<String> {
        self.notes.lock().unwrap().clone()
    }
}

impl br_agentic_contract::PromptPort<Doc> for SnapshotPrompt {
    fn prompt<'a>(
        &'a self,
        snapshot: &'a Doc,
        _label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<String, PortError>> {
        self.notes.lock().unwrap().push(snapshot.note.clone());
        Box::pin(async { Ok("a prompt".to_owned()) })
    }
}
