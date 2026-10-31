use std::fmt::Write;

use br_agentic_contract::MailItem;
use br_llm_messages::{
    AgentId, AgentMail, Author, Error, IncomingEntry, PlainBlock, RuntimeNotice, TextBlock,
};

use crate::ids::block_id_raw;

pub(crate) fn quote(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for character in text.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            control if control.is_control() => {
                let _ = write!(quoted, "\\u{{{:x}}}", u32::from(control));
            }
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

pub(crate) struct MailEntries {
    known: Vec<(AgentId, Author)>,
}

impl MailEntries {
    pub(crate) fn new(known: Vec<(AgentId, Author)>) -> Self {
        Self { known }
    }

    fn agent(&self, agent: AgentId) -> String {
        match self.known.iter().find(|(known, _)| *known == agent) {
            Some((_, author)) => format!("{} (author {})", agent.as_uuid(), quote(author.as_str())),
            None => agent.as_uuid().to_string(),
        }
    }

    fn agents(&self, agents: &[AgentId]) -> String {
        if agents.is_empty() {
            return "none".to_owned();
        }
        agents
            .iter()
            .map(|agent| self.agent(*agent))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub(crate) fn build(&self, item: &MailItem) -> Result<IncomingEntry, Error> {
        match item {
            MailItem::To {
                id,
                sender,
                author,
                source,
                to,
                cc,
                subject,
                body,
                attachment_names,
            } => {
                let names = if attachment_names.is_empty() {
                    "none".to_owned()
                } else {
                    attachment_names
                        .iter()
                        .map(|name| quote(name))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let text = format!(
                    "Mail {id}\nFrom: {} (author {})\nTo: {}\nCc: {}\nSubject: {}\nBody: {}\nAttachments: {names}",
                    sender.as_uuid(),
                    quote(author.as_str()),
                    self.agents(to),
                    self.agents(cc),
                    quote(subject),
                    quote(body),
                );
                let block = TextBlock::new(block_id_raw()?, text)?;
                let mail = AgentMail::new(
                    id.entry_id(),
                    *sender,
                    author.clone(),
                    *source,
                    vec![PlainBlock::Text(block)],
                )?;
                Ok(IncomingEntry::AgentMail(mail))
            }
            MailItem::Cc { id, subject } => {
                let text = format!(
                    "You are in copy of the mail {id}. Subject: {}",
                    quote(subject)
                );
                Ok(IncomingEntry::RuntimeNotice(RuntimeNotice::new(
                    id.entry_id(),
                    text,
                )?))
            }
        }
    }
}
