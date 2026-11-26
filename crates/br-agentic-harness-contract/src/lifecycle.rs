use br_agentic_contract::{InstanceId, PortError, PortFuture};

use crate::seed::Seed;
use crate::start::StartKind;
use crate::turn::TurnRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEnd {
    Completed,
    Failed,
    Cancelled,
}

pub trait RunLifecycle: Send + Sync {
    fn open<'a>(
        &'a self,
        instance: InstanceId,
        turn: TurnRef,
        seed: Seed,
    ) -> PortFuture<'a, Result<StartKind, PortError>>;

    fn close<'a>(
        &'a self,
        instance: InstanceId,
        end: RunEnd,
    ) -> PortFuture<'a, Result<(), PortError>>;
}
