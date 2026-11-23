use std::sync::Arc;

use br_agentic_contract::{
    Attachment, DocumentRef, IncomingContext, MailId, NewMail, PortFuture, ResultStream, ToolCode,
    ToolContent, ToolFault,
};
use br_agentic_harness_contract::MailPort;
use br_graph_contract::CancelView;
use br_llm_messages::{AgentId, Author, ConversationId, EntryId, OpaqueJson};
use serde::Deserialize;
use uuid::Uuid;

use crate::tool_text::{acknowledgement, error, invalid};

#[derive(Deserialize)]
struct AttachmentArgument {
    name: String,
    document: String,
}

#[derive(Deserialize)]
struct SendArguments {
    to: Vec<String>,
    #[serde(default)]
    cc: Vec<String>,
    subject: String,
    body: String,
    #[serde(default)]
    attachments: Vec<AttachmentArgument>,
}

pub(crate) struct SendMail {
    pub(crate) mail: Arc<dyn MailPort>,
    pub(crate) context: IncomingContext,
    pub(crate) author: Author,
    pub(crate) source: ConversationId,
}

fn agent_ids(texts: &[String]) -> Result<Vec<AgentId>, String> {
    texts
        .iter()
        .map(|text| AgentId::parse(text).map_err(|_| format!("{text} is not an agent identity")))
        .collect()
}

impl ToolCode for SendMail {
    fn call<'a>(
        &'a self,
        arguments: &'a OpaqueJson,
        _cancel: CancelView,
        _result: &'a mut dyn ResultStream,
    ) -> PortFuture<'a, Result<ToolContent, ToolFault>> {
        Box::pin(async move {
            let parsed: SendArguments = match arguments.deserialize_into() {
                Ok(parsed) => parsed,
                Err(failure) => return invalid(failure),
            };
            let (to, cc) = match (agent_ids(&parsed.to), agent_ids(&parsed.cc)) {
                (Ok(to), Ok(cc)) => (to, cc),
                (Err(reason), _) | (_, Err(reason)) => return invalid(reason),
            };
            let id = MailId::new(EntryId::from_uuid(Uuid::now_v7()).map_err(ToolFault::new)?);
            let mail = NewMail {
                id,
                author: self.author.clone(),
                source: self.source,
                to,
                cc,
                subject: parsed.subject,
                body: parsed.body,
                attachments: parsed
                    .attachments
                    .into_iter()
                    .map(|a| Attachment::new(a.name, DocumentRef::new(a.document)))
                    .collect(),
            };
            match self.mail.send(&self.context, mail).await {
                Ok(()) => acknowledgement(format!("the mail {id} was sent")),
                Err(failure) => match failure.reason() {
                    Some(reason) => error(format!("the mail was refused: {reason}")),
                    None => error("the mail could not be sent now"),
                },
            }
        })
    }
}
