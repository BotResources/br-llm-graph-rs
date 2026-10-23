use std::fmt;

use uuid::{Uuid, Variant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InstanceId(Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("instance id {0} is not a UUIDv7")]
pub struct InvalidInstanceId(Uuid);

impl InstanceId {
    pub fn new(value: Uuid) -> Result<Self, InvalidInstanceId> {
        if value.get_version_num() == 7 && value.get_variant() == Variant::RFC4122 {
            Ok(Self(value))
        } else {
            Err(InvalidInstanceId(value))
        }
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl TryFrom<Uuid> for InstanceId {
    type Error = InvalidInstanceId;

    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<InstanceId> for Uuid {
    fn from(id: InstanceId) -> Self {
        id.0
    }
}

impl fmt::Display for InstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
