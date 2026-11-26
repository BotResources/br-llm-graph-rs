use br_agentic_contract::{InstanceId, WebAccess};

use crate::sandbox::BoxId;
use crate::seed::Seed;
use crate::start::StartKind;
use crate::turn::TurnRef;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct OpenRequest {
    pub instance: InstanceId,
    pub web_access: Option<WebAccess>,
    pub turn: TurnRef,
    pub seed: Seed,
}

impl OpenRequest {
    pub fn new(
        instance: InstanceId,
        web_access: Option<WebAccess>,
        turn: TurnRef,
        seed: Seed,
    ) -> Self {
        Self {
            instance,
            web_access,
            turn,
            seed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Opened {
    pub box_id: BoxId,
    pub start: StartKind,
}

impl Opened {
    pub fn new(box_id: BoxId, start: StartKind) -> Self {
        Self { box_id, start }
    }
}
