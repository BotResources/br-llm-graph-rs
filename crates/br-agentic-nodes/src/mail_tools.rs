use std::sync::Arc;

use br_agentic_contract::{
    Attachment, DocumentRef, InboxContext, InboxPort, MailDocument, MailId, NewMail, PortError,
    PortFuture, ResultStream, Tool, ToolBlock, ToolCode, ToolContent, ToolFault,
};
use br_graph_contract::CancelView;
use br_llm_clients_core::{Parameter, ParameterKind, Parameters, ToolSpec};
use br_llm_messages::{AgentId, Author, ConversationId, EntryId, OpaqueJson, ToolName};
use serde::Deserialize;

use crate::fault::Fault;
use crate::ids::entry_id;
use crate::worker::MailGate;

pub const SEND_MAIL: &str = "send_mail";
pub const GET_MAIL_FILES: &str = "get_mail_files";

pub fn mail_tools(
    inbox: Arc<dyn InboxPort>,
    context: InboxContext,
    author: Author,
    source: ConversationId,
    gate: MailGate,
) -> Result<Vec<Tool>, Fault> {
    let send = Tool::new(
        send_mail_spec()?,
        Arc::new(SendMail {
            inbox: Arc::clone(&inbox),
            context,
            author,
            source,
        }),
    );
    let files = Tool::new(
        get_mail_files_spec()?,
        Arc::new(GetMailFiles {
            inbox,
            context,
            gate,
        }),
    );
    Ok(vec![send, files])
}

fn parameter(
    name: &str,
    description: &str,
    required: bool,
    kind: ParameterKind,
) -> Result<Parameter, Fault> {
    Parameter::new(name, description, required, kind).map_err(Fault::value)
}

fn parameters(list: Vec<Parameter>) -> Result<Parameters, Fault> {
    Parameters::new(list).map_err(Fault::value)
}

fn agents() -> ParameterKind {
    ParameterKind::Array {
        items: Box::new(ParameterKind::String),
    }
}

fn send_mail_spec() -> Result<ToolSpec, Fault> {
    let attachment = parameters(vec![
        parameter(
            "name",
            "The name of the attachment.",
            true,
            ParameterKind::String,
        )?,
        parameter(
            "document",
            "The reference of the document to attach.",
            true,
            ParameterKind::String,
        )?,
    ])?;
    let list = parameters(vec![
        parameter(
            "to",
            "The identities of the agents that the mail is for. At least one.",
            true,
            agents(),
        )?,
        parameter(
            "cc",
            "The identities of the agents that get the mail in copy.",
            false,
            agents(),
        )?,
        parameter(
            "subject",
            "The subject of the mail.",
            true,
            ParameterKind::String,
        )?,
        parameter("body", "The body of the mail.", true, ParameterKind::String)?,
        parameter(
            "attachments",
            "The documents to attach to the mail.",
            false,
            ParameterKind::Array {
                items: Box::new(ParameterKind::Object {
                    parameters: attachment,
                }),
            },
        )?,
    ])?;
    ToolSpec::new(
        ToolName::new(SEND_MAIL).map_err(Fault::value)?,
        "Send a mail to other agents. The answer comes later, as a mail.",
        list,
    )
    .map_err(Fault::value)
}

fn get_mail_files_spec() -> Result<ToolSpec, Fault> {
    let list = parameters(vec![parameter(
        "mail_id",
        "The identity of a mail that you received.",
        true,
        ParameterKind::String,
    )?])?;
    ToolSpec::new(
        ToolName::new(GET_MAIL_FILES).map_err(Fault::value)?,
        "Get the documents that are attached to a mail you received.",
        list,
    )
    .map_err(Fault::value)
}

fn error(message: impl Into<String>) -> Result<ToolContent, ToolFault> {
    ToolContent::error_text(message).map_err(ToolFault::new)
}

fn acknowledgement(message: impl Into<String>) -> Result<ToolContent, ToolFault> {
    let block = ToolBlock::text(message).map_err(ToolFault::new)?;
    Ok(ToolContent::new(vec![block], false))
}

fn invalid(reason: impl std::fmt::Display) -> Result<ToolContent, ToolFault> {
    error(format!("the arguments are invalid: {reason}"))
}

fn inbox_error(error_value: &PortError) -> Result<ToolContent, ToolFault> {
    match error_value.reason() {
        Some(reason) => error(format!("the inbox refused the request: {reason}")),
        None => error("the inbox could not do it now"),
    }
}

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

#[derive(Deserialize)]
struct FilesArguments {
    mail_id: String,
}

struct SendMail {
    inbox: Arc<dyn InboxPort>,
    context: InboxContext,
    author: Author,
    source: ConversationId,
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
            let id = MailId::new(entry_id().map_err(ToolFault::new)?);
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
            match self.inbox.send(&self.context, mail).await {
                Ok(()) => acknowledgement(format!("the mail {id} was sent")),
                Err(failure) => match failure.reason() {
                    Some(reason) => error(format!("the mail was refused: {reason}")),
                    None => error("the mail could not be sent now"),
                },
            }
        })
    }
}

struct GetMailFiles {
    inbox: Arc<dyn InboxPort>,
    context: InboxContext,
    gate: MailGate,
}

fn document_block(document: &MailDocument) -> Result<ToolBlock, ToolFault> {
    let text = match std::str::from_utf8(&document.content) {
        Ok(content) => format!("{}:\n{content}", document.name),
        Err(_) => format!(
            "{}: a document of {} bytes that is not text",
            document.name,
            document.content.len()
        ),
    };
    ToolBlock::text(text).map_err(ToolFault::new)
}

impl ToolCode for GetMailFiles {
    fn call<'a>(
        &'a self,
        arguments: &'a OpaqueJson,
        _cancel: CancelView,
        _result: &'a mut dyn ResultStream,
    ) -> PortFuture<'a, Result<ToolContent, ToolFault>> {
        Box::pin(async move {
            let parsed: FilesArguments = match arguments.deserialize_into() {
                Ok(parsed) => parsed,
                Err(failure) => return invalid(failure),
            };
            let Ok(entry) = EntryId::parse(&parsed.mail_id) else {
                return invalid(format!("{} is not a mail identity", parsed.mail_id));
            };
            let held = self
                .gate
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains(&entry);
            if !held {
                return error(format!(
                    "the mail {entry} is not in your conversation: you cannot get its files"
                ));
            }
            let documents = match self
                .inbox
                .documents(&self.context, MailId::new(entry))
                .await
            {
                Ok(documents) => documents,
                Err(failure) => return inbox_error(&failure),
            };
            let blocks = documents
                .iter()
                .map(document_block)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ToolContent::new(blocks, false))
        })
    }
}
