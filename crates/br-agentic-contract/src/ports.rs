use br_graph_contract::{Fields, InstanceId};
use br_llm_clients_core::BoxFuture;
use br_llm_messages::{AgentId, Author};

use crate::fault::PortFault;
use crate::ids::{AgentLabel, ThreadRef};
use crate::tool::Tool;
use crate::workspace::WorkspaceFile;

#[derive(Debug, Clone, PartialEq)]
pub struct Agent {
    pub id: AgentId,
    pub author: Option<Author>,
}

pub trait ThreadPort<F: Fields>: Send + Sync {
    fn thread<'a>(&'a self, snapshot: &'a F) -> BoxFuture<'a, Result<ThreadRef, PortFault>>;
}

pub trait AgentPort<F: Fields>: Send + Sync {
    fn agent<'a>(
        &'a self,
        snapshot: &'a F,
        instance: InstanceId,
        label: &'a AgentLabel,
    ) -> BoxFuture<'a, Result<Agent, PortFault>>;
}

pub trait ToolsPort<F: Fields>: Send + Sync {
    fn tools<'a>(
        &'a self,
        snapshot: &'a F,
        instance: InstanceId,
    ) -> BoxFuture<'a, Result<Vec<Tool>, PortFault>>;
}

pub trait WorkspacePort<F: Fields>: Send + Sync {
    fn load<'a>(&'a self, snapshot: &'a F) -> BoxFuture<'a, Result<Vec<WorkspaceFile>, PortFault>>;

    fn save<'a>(
        &'a self,
        snapshot: &'a F,
        files: Vec<WorkspaceFile>,
    ) -> BoxFuture<'a, Result<(), PortFault>>;
}
