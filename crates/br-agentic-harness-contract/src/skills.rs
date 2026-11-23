use std::fmt;

use br_agentic_contract::{InstanceId, PortError, PortFuture};

use crate::path::FilePath;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SkillName(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a skill name is a non-blank text")]
pub struct InvalidSkillName;

impl SkillName {
    pub fn new(text: impl Into<String>) -> Result<Self, InvalidSkillName> {
        let text = text.into();
        if text.trim().is_empty() {
            return Err(InvalidSkillName);
        }
        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SkillName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSummary {
    pub name: SkillName,
    pub description: String,
}

pub trait SkillsPort: Send + Sync {
    fn list<'a>(
        &'a self,
        instance: InstanceId,
    ) -> PortFuture<'a, Result<Vec<SkillSummary>, PortError>>;

    fn read<'a>(
        &'a self,
        instance: InstanceId,
        skill: &'a SkillName,
        path: &'a FilePath,
    ) -> PortFuture<'a, Result<Vec<u8>, PortError>>;
}
