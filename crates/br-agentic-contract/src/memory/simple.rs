use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use br_graph_contract::{Fields, InstanceId};
use br_llm_clients_core::BoxFuture;
use br_llm_messages::{AgentId, Author, Uuid};

use super::state::MemoryHub;
use crate::fault::PortFault;
use crate::ids::{AgentLabel, ThreadRef};
use crate::ports::{Agent, AgentPort, ThreadPort, ToolsPort, WorkspacePort};
use crate::tool::Tool;
use crate::workspace::WorkspaceFile;

pub struct FixedThreadPort {
    thread: ThreadRef,
}

impl FixedThreadPort {
    pub fn new(thread: ThreadRef) -> Self {
        Self { thread }
    }
}

impl<F: Fields> ThreadPort<F> for FixedThreadPort {
    fn thread<'a>(&'a self, _snapshot: &'a F) -> BoxFuture<'a, Result<ThreadRef, PortFault>> {
        Box::pin(async move { Ok(self.thread) })
    }
}

pub struct FixedToolsPort {
    tools: Vec<Tool>,
}

impl FixedToolsPort {
    pub fn new(tools: Vec<Tool>) -> Self {
        Self { tools }
    }
}

impl<F: Fields> ToolsPort<F> for FixedToolsPort {
    fn tools<'a>(
        &'a self,
        _snapshot: &'a F,
        _instance: InstanceId,
    ) -> BoxFuture<'a, Result<Vec<Tool>, PortFault>> {
        Box::pin(async move { Ok(self.tools.clone()) })
    }
}

pub struct MemoryAgentPort<F: Fields> {
    hub: MemoryHub,
    authors: HashMap<AgentLabel, Author>,
    saved: Mutex<HashMap<AgentLabel, AgentId>>,
    marker: PhantomData<fn(&F)>,
}

impl<F: Fields> MemoryAgentPort<F> {
    pub fn new(hub: MemoryHub) -> Self {
        Self {
            hub,
            authors: HashMap::new(),
            saved: Mutex::new(HashMap::new()),
            marker: PhantomData,
        }
    }

    pub fn with_author(mut self, label: AgentLabel, author: Author) -> Self {
        self.authors.insert(label, author);
        self
    }
}

impl<F: Fields> AgentPort<F> for MemoryAgentPort<F> {
    fn agent<'a>(
        &'a self,
        _snapshot: &'a F,
        _instance: InstanceId,
        label: &'a AgentLabel,
    ) -> BoxFuture<'a, Result<Agent, PortFault>> {
        Box::pin(async move {
            let author = self.authors.get(label).cloned();
            let mut saved = self.saved.lock().unwrap_or_else(PoisonError::into_inner);
            let id = match saved.get(label) {
                Some(id) => *id,
                None => {
                    let id = AgentId::from_uuid(Uuid::now_v7())
                        .map_err(|error| PortFault::new(error.to_string()))?;
                    saved.insert(label.clone(), id);
                    id
                }
            };
            if let Some(author) = &author {
                self.hub.set_author(id, author.clone());
            }
            Ok(Agent { id, author })
        })
    }
}

pub struct MemoryWorkspace {
    files: Mutex<Vec<WorkspaceFile>>,
    loads: AtomicUsize,
    saves: Arc<AtomicUsize>,
}

impl MemoryWorkspace {
    pub fn new(files: Vec<WorkspaceFile>) -> Self {
        Self {
            files: Mutex::new(files),
            loads: AtomicUsize::new(0),
            saves: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn loads(&self) -> usize {
        self.loads.load(Ordering::SeqCst)
    }

    pub fn saves(&self) -> usize {
        self.saves.load(Ordering::SeqCst)
    }

    pub fn files(&self) -> Vec<WorkspaceFile> {
        self.files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl<F: Fields> WorkspacePort<F> for MemoryWorkspace {
    fn load<'a>(
        &'a self,
        _snapshot: &'a F,
    ) -> BoxFuture<'a, Result<Vec<WorkspaceFile>, PortFault>> {
        Box::pin(async move {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(self.files())
        })
    }

    fn save<'a>(
        &'a self,
        _snapshot: &'a F,
        files: Vec<WorkspaceFile>,
    ) -> BoxFuture<'a, Result<(), PortFault>> {
        Box::pin(async move {
            self.saves.fetch_add(1, Ordering::SeqCst);
            *self.files.lock().unwrap_or_else(PoisonError::into_inner) = files;
            Ok(())
        })
    }
}
