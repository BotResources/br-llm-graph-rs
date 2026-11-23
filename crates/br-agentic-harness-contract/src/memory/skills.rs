use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use br_agentic_contract::{InstanceId, PortError, PortFuture};

use super::record::{HarnessCall, HarnessRecord};
use crate::path::FilePath;
use crate::skills::{SkillName, SkillSummary, SkillsPort};

struct Skill {
    description: String,
    files: BTreeMap<FilePath, Vec<u8>>,
}

#[derive(Clone)]
pub struct MemorySkills {
    record: HarnessRecord,
    skills: Arc<Mutex<BTreeMap<SkillName, Skill>>>,
}

impl MemorySkills {
    pub fn new(record: &HarnessRecord) -> Self {
        Self {
            record: record.clone(),
            skills: Arc::default(),
        }
    }

    pub fn with_skill(
        self,
        name: SkillName,
        description: impl Into<String>,
        files: Vec<(FilePath, Vec<u8>)>,
    ) -> Self {
        self.skills
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                name,
                Skill {
                    description: description.into(),
                    files: files.into_iter().collect(),
                },
            );
        self
    }
}

impl SkillsPort for MemorySkills {
    fn list<'a>(
        &'a self,
        instance: InstanceId,
    ) -> PortFuture<'a, Result<Vec<SkillSummary>, PortError>> {
        self.record.push(HarnessCall::SkillsList { instance });
        Box::pin(async move {
            let skills = self.skills.lock().unwrap_or_else(PoisonError::into_inner);
            Ok(skills
                .iter()
                .map(|(name, skill)| SkillSummary {
                    name: name.clone(),
                    description: skill.description.clone(),
                })
                .collect())
        })
    }

    fn read<'a>(
        &'a self,
        instance: InstanceId,
        skill: &'a SkillName,
        path: &'a FilePath,
    ) -> PortFuture<'a, Result<Vec<u8>, PortError>> {
        self.record.push(HarnessCall::SkillsRead {
            instance,
            skill: skill.clone(),
            path: path.clone(),
        });
        Box::pin(async move {
            let skills = self.skills.lock().unwrap_or_else(PoisonError::into_inner);
            let held = skills
                .get(skill)
                .ok_or_else(|| PortError::refused(format!("the skill {skill} does not exist")))?;
            held.files
                .get(path)
                .cloned()
                .ok_or_else(|| PortError::refused(format!("the skill {skill} has no file {path}")))
        })
    }
}
