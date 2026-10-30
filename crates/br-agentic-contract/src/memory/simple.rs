use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::{Mutex, PoisonError};

use br_graph_contract::{Fields, InstanceId};
use br_llm_messages::{AgentId, Uuid};

use super::record::{Call, Record};
use crate::error::PortError;
use crate::future::PortFuture;
use crate::ids::{AgentLabel, ThreadRef};
use crate::ports::{AgentPort, ThreadPort, ToolsPort};
use crate::tool::Tool;
use crate::workspace::{WorkspaceFile, WorkspacePort};

pub struct FixedThreadPort {
    record: Record,
    thread: ThreadRef,
}

impl FixedThreadPort {
    pub fn new(record: &Record, thread: ThreadRef) -> Self {
        Self {
            record: record.clone(),
            thread,
        }
    }
}

impl<F: Fields> ThreadPort<F> for FixedThreadPort {
    fn thread<'a>(&'a self, _snapshot: &'a F) -> PortFuture<'a, Result<ThreadRef, PortError>> {
        self.record.push(Call::ThreadPort);
        Box::pin(async move { Ok(self.thread) })
    }
}

pub struct FixedToolsPort {
    record: Record,
    tools: Vec<Tool>,
}

impl FixedToolsPort {
    pub fn new(record: &Record, tools: Vec<Tool>) -> Self {
        Self {
            record: record.clone(),
            tools,
        }
    }
}

impl<F: Fields> ToolsPort<F> for FixedToolsPort {
    fn tools<'a>(
        &'a self,
        _snapshot: &'a F,
        instance: InstanceId,
    ) -> PortFuture<'a, Result<Vec<Tool>, PortError>> {
        self.record.push(Call::ToolsPort { instance });
        Box::pin(async move { Ok(self.tools.clone()) })
    }
}

pub struct MemoryAgentPort<F: Fields> {
    record: Record,
    agents: Mutex<HashMap<AgentLabel, AgentId>>,
    marker: PhantomData<fn(&F)>,
}

impl<F: Fields> MemoryAgentPort<F> {
    pub fn new(record: &Record) -> Self {
        Self {
            record: record.clone(),
            agents: Mutex::new(HashMap::new()),
            marker: PhantomData,
        }
    }

    pub fn with_agent(self, label: AgentLabel, agent: AgentId) -> Self {
        self.agents
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(label, agent);
        self
    }
}

impl<F: Fields> AgentPort<F> for MemoryAgentPort<F> {
    fn agent<'a>(
        &'a self,
        _snapshot: &'a F,
        instance: InstanceId,
        label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<AgentId, PortError>> {
        self.record.push(Call::AgentPort {
            instance,
            label: label.clone(),
        });
        Box::pin(async move {
            let mut agents = self.agents.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(agent) = agents.get(label) {
                return Ok(*agent);
            }
            let agent = AgentId::from_uuid(Uuid::now_v7()).map_err(PortError::fault)?;
            agents.insert(label.clone(), agent);
            Ok(agent)
        })
    }
}

pub struct MemoryWorkspace {
    record: Record,
    files: Mutex<Vec<WorkspaceFile>>,
}

impl MemoryWorkspace {
    pub fn new(record: &Record, files: Vec<WorkspaceFile>) -> Self {
        Self {
            record: record.clone(),
            files: Mutex::new(files),
        }
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
    ) -> PortFuture<'a, Result<Vec<WorkspaceFile>, PortError>> {
        self.record.push(Call::WorkspaceLoad);
        Box::pin(async move { Ok(self.files()) })
    }

    fn save<'a>(
        &'a self,
        _snapshot: &'a F,
        files: Vec<WorkspaceFile>,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(Call::WorkspaceSave {
            files: files.clone(),
        });
        Box::pin(async move {
            *self.files.lock().unwrap_or_else(PoisonError::into_inner) = files;
            Ok(())
        })
    }
}
