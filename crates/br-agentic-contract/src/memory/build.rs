use br_graph_contract::Reason;
use br_llm_messages::{
    AgentId, AgentMail, BlockId, Conversation, Error, IncomingEntry, PlainBlock, RuntimeNotice,
    TextBlock, Uuid,
};

use super::state::State;
use crate::fault::{PortFault, ThreadError};
use crate::mail::MailItem;
use crate::thread_access::Incoming;

pub(super) fn refused(error: Error) -> ThreadError {
    ThreadError::Refused(Reason::new(error.to_string()))
}

fn author_of(state: &State, agent: AgentId) -> Result<br_llm_messages::Author, ThreadError> {
    state.authors.get(&agent).cloned().ok_or_else(|| {
        ThreadError::Fault(PortFault::new(format!(
            "no author name is known for agent {}",
            agent.as_uuid()
        )))
    })
}

fn text_block(text: String) -> Result<PlainBlock, ThreadError> {
    let id = BlockId::from_uuid(Uuid::now_v7()).map_err(refused)?;
    TextBlock::new(id, text)
        .map(PlainBlock::Text)
        .map_err(refused)
}

fn recipients(agents: &[AgentId]) -> String {
    agents
        .iter()
        .map(|agent| agent.as_uuid().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn incoming_entry(
    state: &State,
    conversation: &Conversation,
    item: &Incoming,
) -> Result<IncomingEntry, ThreadError> {
    match item {
        Incoming::Mail(MailItem::To {
            id,
            sender,
            to,
            cc,
            subject,
            body,
            ..
        }) => {
            let text = format!(
                "Subject: {subject}\nTo: {}\nCc: {}\n\n{body}",
                recipients(to),
                recipients(cc)
            );
            let source = state
                .sources
                .get(sender)
                .copied()
                .unwrap_or_else(|| conversation.id());
            AgentMail::new(
                id.entry_id(),
                *sender,
                author_of(state, *sender)?,
                source,
                vec![text_block(text)?],
            )
            .map(IncomingEntry::from)
            .map_err(refused)
        }
        Incoming::Mail(MailItem::Cc { id, subject }) => RuntimeNotice::new(
            id.entry_id(),
            format!("Mail {id} was sent in copy: {subject}"),
        )
        .map(IncomingEntry::from)
        .map_err(refused),
        Incoming::Entry(entry) => Ok(entry.entry().clone()),
        Incoming::Written(written) => {
            let source = state
                .sources
                .get(&written.sender)
                .copied()
                .unwrap_or_else(|| conversation.id());
            AgentMail::new(
                written.id,
                written.sender,
                written.author.clone(),
                source,
                written.content.clone(),
            )
            .map(IncomingEntry::from)
            .map_err(refused)
        }
    }
}
