use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use br_agentic_contract::memory::{FixedThreadPort, FixedToolsPort, Record};
use br_agentic_contract::{AgentLabel, PortError, PortFuture, ThreadAccess, Tool, ToolsPort};
use br_agentic_nodes::{
    AuthorPort, AuthorSource, FallbackList, Ports, PromptPort, ReactAgent, ReactOutcome,
    ReactOutcomePort, ReactSpec,
};
use br_graph_contract::InstanceId;

use crate::common::{Doc, Recorded, Rig, names, text_of};

pub struct Seen {
    pub note: String,
    pub outcome: ReactOutcome,
    pub before: Vec<&'static str>,
}

pub struct Capture {
    record: Record,
    pub seen: Mutex<Vec<Seen>>,
    attempts: AtomicUsize,
    fail: bool,
}

impl Capture {
    pub fn new(record: &Record) -> Arc<Self> {
        Arc::new(Self {
            record: record.clone(),
            seen: Mutex::new(Vec::new()),
            attempts: AtomicUsize::new(0),
            fail: false,
        })
    }

    pub fn failing(record: &Record) -> Arc<Self> {
        Arc::new(Self {
            record: record.clone(),
            seen: Mutex::new(Vec::new()),
            attempts: AtomicUsize::new(0),
            fail: true,
        })
    }

    pub fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }

    pub fn only(&self) -> ReactOutcome {
        let seen = self.seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "the outcome port is called once");
        seen[0].outcome.clone()
    }
}

impl ReactOutcomePort<Doc> for Capture {
    fn update<'a>(
        &'a self,
        snapshot: &'a Doc,
        outcome: &'a ReactOutcome,
    ) -> PortFuture<'a, Result<Recorded, PortError>> {
        Box::pin(async move {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(PortError::fault("the outcome port failed"));
            }
            self.seen.lock().unwrap().push(Seen {
                note: snapshot.note.clone(),
                outcome: outcome.clone(),
                before: names(&self.record),
            });
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

#[derive(Default)]
pub struct SnapPrompt {
    pub seen: Mutex<Vec<(String, String)>>,
}

impl PromptPort<Doc> for SnapPrompt {
    fn prompt<'a>(
        &'a self,
        snapshot: &'a Doc,
        label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<String, PortError>> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap()
                .push((snapshot.note.clone(), label.as_str().to_owned()));
            Ok("snapshot prompt".to_owned())
        })
    }
}

pub struct SnapAuthor {
    name: String,
    pub seen: Mutex<Vec<(String, String)>>,
}

impl SnapAuthor {
    pub fn new(name: &str) -> Arc<Self> {
        Arc::new(Self {
            name: name.to_owned(),
            seen: Mutex::new(Vec::new()),
        })
    }
}

impl AuthorPort<Doc> for SnapAuthor {
    fn author<'a>(
        &'a self,
        snapshot: &'a Doc,
        label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<String, PortError>> {
        Box::pin(async move {
            self.seen
                .lock()
                .unwrap()
                .push((snapshot.note.clone(), label.as_str().to_owned()));
            Ok(self.name.clone())
        })
    }
}

pub struct SnapTools {
    inner: FixedToolsPort,
    pub seen: Mutex<Vec<String>>,
}

impl SnapTools {
    pub fn new(record: &Record, tools: Vec<Tool>) -> Arc<Self> {
        Arc::new(Self {
            inner: FixedToolsPort::new(record, tools),
            seen: Mutex::new(Vec::new()),
        })
    }
}

impl ToolsPort<Doc> for SnapTools {
    fn tools<'a>(
        &'a self,
        snapshot: &'a Doc,
        instance: InstanceId,
    ) -> PortFuture<'a, Result<Vec<Tool>, PortError>> {
        self.seen.lock().unwrap().push(snapshot.note.clone());
        ToolsPort::<Doc>::tools(&self.inner, snapshot, instance)
    }
}

pub fn node(rig: &Rig, fallback: FallbackList) -> ReactAgent<Doc> {
    ReactAgent::new(
        "agent",
        rig.ports(),
        rig.settings.clone(),
        rig.react_spec(fallback),
    )
}

pub fn node_capturing(
    rig: &Rig,
    fallback: FallbackList,
    capture: &Arc<Capture>,
) -> ReactAgent<Doc> {
    let mut spec: ReactSpec<Doc> = rig.react_spec(fallback);
    spec.outcome = capture.clone();
    ReactAgent::new("agent", rig.ports(), rig.settings.clone(), spec)
}

pub fn ports_over(rig: &Rig, access: Arc<dyn ThreadAccess>) -> Ports<Doc> {
    Ports::new(
        Arc::new(FixedThreadPort::new(&rig.record, rig.thread)),
        rig.agents.clone(),
        access,
        rig.prompts.clone(),
        Arc::new(FixedToolsPort::new(&rig.record, rig.tools.clone())),
        rig.workspace.clone(),
    )
}

pub fn node_over(
    rig: &Rig,
    access: Arc<dyn ThreadAccess>,
    fallback: FallbackList,
) -> ReactAgent<Doc> {
    ReactAgent::new(
        "agent",
        ports_over(rig, access),
        rig.settings.clone(),
        rig.react_spec(fallback),
    )
}

pub fn give_note(driver: &crate::common::Driver, note: &str) {
    let state = br_graph_contract::State::create(Doc {
        note: note.to_owned(),
    })
    .unwrap();
    let _ = driver
        .input
        .give(br_graph_contract::Input::new(Arc::new(state)));
}

pub fn node_with_ports(
    rig: &Rig,
    prompt: Arc<dyn PromptPort<Doc>>,
    tools: Arc<dyn ToolsPort<Doc>>,
    author: AuthorSource<Doc>,
    outcome: Arc<dyn ReactOutcomePort<Doc>>,
    fallback: FallbackList,
) -> ReactAgent<Doc> {
    let ports = Ports::new(
        Arc::new(FixedThreadPort::new(&rig.record, rig.thread)),
        rig.agents.clone(),
        rig.access.clone(),
        prompt,
        tools,
        rig.workspace.clone(),
    );
    let spec = ReactSpec {
        fallback,
        outcome,
        author,
    };
    ReactAgent::new("agent", ports, rig.settings.clone(), spec)
}

impl SnapTools {
    pub fn into_port(self: Arc<Self>) -> SnapToolsPort {
        SnapToolsPort(self)
    }
}

pub struct SnapToolsPort(Arc<SnapTools>);

impl ToolsPort<Doc> for SnapToolsPort {
    fn tools<'a>(
        &'a self,
        snapshot: &'a Doc,
        instance: InstanceId,
    ) -> PortFuture<'a, Result<Vec<Tool>, PortError>> {
        ToolsPort::<Doc>::tools(&*self.0, snapshot, instance)
    }
}
