use std::fmt;

use br_llm_messages::{EntryId, Error, Uuid};

const UUID_V7: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThreadRef(Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a thread reference is a UUIDv7")]
pub struct InvalidThreadRef;

impl ThreadRef {
    pub fn from_uuid(uuid: Uuid) -> Result<Self, InvalidThreadRef> {
        if uuid.get_version_num() == UUID_V7 {
            Ok(Self(uuid))
        } else {
            Err(InvalidThreadRef)
        }
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl fmt::Display for ThreadRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MailId(EntryId);

impl MailId {
    pub fn new(entry: EntryId) -> Self {
        Self(entry)
    }

    pub fn from_uuid(uuid: Uuid) -> Result<Self, Error> {
        EntryId::from_uuid(uuid).map(Self)
    }

    pub fn entry_id(&self) -> EntryId {
        self.0
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0.as_uuid()
    }
}

impl From<MailId> for EntryId {
    fn from(id: MailId) -> Self {
        id.0
    }
}

impl fmt::Display for MailId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_uuid().fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentLabel(String);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("an agent label is a non-empty text")]
pub struct InvalidLabel;

impl AgentLabel {
    pub fn new(text: impl Into<String>) -> Result<Self, InvalidLabel> {
        let text = text.into();
        if text.is_empty() {
            return Err(InvalidLabel);
        }
        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AgentLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
