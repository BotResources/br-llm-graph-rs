use br_agentic_contract::NewMail;
use br_agentic_contract::memory::MemoryInbox;
use br_llm_messages::{AgentId, ConversationId};

use crate::prelude::*;

pub fn deliver_mails(inbox: &MemoryInbox, generator: AgentId) {
    let sender = agent_id();
    let source = ConversationId::from_uuid(uuid()).unwrap();
    inbox
        .deliver_mail(
            sender,
            NewMail {
                id: mail_id(),
                author: author("Sender"),
                source,
                to: vec![generator],
                cc: Vec::new(),
                subject: "SUBJECT-TO".to_owned(),
                body: "BODY-TO".to_owned(),
                attachments: Vec::new(),
            },
        )
        .unwrap();
    inbox
        .deliver_mail(
            sender,
            NewMail {
                id: mail_id(),
                author: author("Sender"),
                source,
                to: vec![agent_id()],
                cc: vec![generator],
                subject: "SUBJECT-CC".to_owned(),
                body: "BODY-CC".to_owned(),
                attachments: Vec::new(),
            },
        )
        .unwrap();
}

pub const MAIL_MARKERS: [&str; 3] = ["SUBJECT-TO", "BODY-TO", "SUBJECT-CC"];
