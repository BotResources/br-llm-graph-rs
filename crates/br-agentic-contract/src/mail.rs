use std::collections::HashSet;

use br_graph_contract::Reason;
use br_llm_messages::AgentId;

use crate::ids::MailId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocumentRef(String);

impl DocumentRef {
    pub fn new(reference: impl Into<String>) -> Self {
        Self(reference.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    name: String,
    document: DocumentRef,
}

impl Attachment {
    pub fn new(name: impl Into<String>, document: DocumentRef) -> Self {
        Self {
            name: name.into(),
            document,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn document(&self) -> &DocumentRef {
        &self.document
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailDocument {
    pub name: String,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMail {
    pub id: MailId,
    pub to: Vec<AgentId>,
    pub cc: Vec<AgentId>,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<Attachment>,
}

impl NewMail {
    pub fn refusal(&self) -> Option<Reason> {
        if self.to.is_empty() {
            return Some(Reason::new("a mail needs at least one recipient in to"));
        }
        let mut seen = HashSet::new();
        self.to
            .iter()
            .chain(self.cc.iter())
            .find(|recipient| !seen.insert(**recipient))
            .map(|recipient| {
                Reason::new(format!(
                    "the recipient {} is listed twice",
                    recipient.as_uuid()
                ))
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailItem {
    To {
        id: MailId,
        sender: AgentId,
        to: Vec<AgentId>,
        cc: Vec<AgentId>,
        subject: String,
        body: String,
        attachment_names: Vec<String>,
    },
    Cc {
        id: MailId,
        subject: String,
    },
}

impl MailItem {
    pub fn id(&self) -> MailId {
        match self {
            MailItem::To { id, .. } | MailItem::Cc { id, .. } => *id,
        }
    }

    pub fn subject(&self) -> &str {
        match self {
            MailItem::To { subject, .. } | MailItem::Cc { subject, .. } => subject,
        }
    }
}
