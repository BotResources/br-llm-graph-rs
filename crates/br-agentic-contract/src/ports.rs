use br_graph_contract::{Fields, InstanceId};
use br_llm_messages::AgentId;

use crate::error::PortError;
use crate::future::PortFuture;
use crate::ids::{AgentLabel, ThreadRef};
use crate::tool::Tool;

pub trait ThreadPort<F: Fields>: Send + Sync {
    fn thread<'a>(&'a self, snapshot: &'a F) -> PortFuture<'a, Result<ThreadRef, PortError>>;
}

pub trait AgentPort<F: Fields>: Send + Sync {
    fn agent<'a>(
        &'a self,
        snapshot: &'a F,
        instance: InstanceId,
        label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<AgentId, PortError>>;
}

pub trait ToolsPort<F: Fields>: Send + Sync {
    fn tools<'a>(
        &'a self,
        snapshot: &'a F,
        instance: InstanceId,
    ) -> PortFuture<'a, Result<Vec<Tool>, PortError>>;
}
