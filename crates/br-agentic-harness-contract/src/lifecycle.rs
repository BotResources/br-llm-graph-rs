use br_agentic_contract::{InstanceId, PortError, PortFuture};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    Completed,
    Failed,
    Cancelled,
}

pub trait RunLifecycle: Send + Sync {
    fn open<'a>(&'a self, instance: InstanceId) -> PortFuture<'a, Result<(), PortError>>;

    fn close<'a>(
        &'a self,
        instance: InstanceId,
        end: RunEnd,
    ) -> PortFuture<'a, Result<(), PortError>>;
}
