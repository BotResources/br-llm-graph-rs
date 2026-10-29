use std::sync::Arc;

use br_agentic_contract::{
    Attachment, DocumentRef, InboxContext, InboxError, InboxPort, MailDocument, MailId, NewMail,
    ResultStream, ThreadAccess, Tool, ToolCode, ToolContent, ToolFault,
};
use br_graph_contract::CancelView;
use br_llm_clients_core::{BoxFuture, Parameter, ParameterKind, Parameters, ToolSpec};
use br_llm_messages::{AgentId, EntryId, OpaqueJson, PlainBlock, TextBlock, ToolName};
use serde::Deserialize;

use crate::fault::Fault;
use crate::ids::{block_id, entry_id};

pub const SEND_MAIL: &str = "send_mail";
pub const GET_MAIL_FILES: &str = "get_mail_files";

pub fn mail_tools(
    inbox: Arc<dyn InboxPort>,
    access: Arc<dyn ThreadAccess>,
    context: InboxContext,
) -> Result<Vec<Tool>, Fault> {
    let send = Tool::new(
        send_mail_spec()?,
        Arc::new(SendMail {
            inbox: Arc::clone(&inbox),
            context,
        }),
    );
    let files = Tool::new(
        get_mail_files_spec()?,
        Arc::new(GetMailFiles {
            inbox,
            access,
            context,
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

fn text(message: impl Into<String>, is_error: bool) -> Result<ToolContent, ToolFault> {
    let block = block_id().map_err(ToolFault::new)?;
    let text = TextBlock::new(block, message).map_err(ToolFault::new)?;
    Ok(ToolContent::new(vec![PlainBlock::Text(text)], is_error))
}

fn invalid(reason: impl std::fmt::Display) -> Result<ToolContent, ToolFault> {
    text(format!("the arguments are invalid: {reason}"), true)
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
    ) -> BoxFuture<'a, Result<ToolContent, ToolFault>> {
        Box::pin(async move {
            let parsed: SendArguments = match arguments.deserialize_into() {
                Ok(parsed) => parsed,
                Err(error) => return invalid(error),
            };
            let (to, cc) = match (agent_ids(&parsed.to), agent_ids(&parsed.cc)) {
                (Ok(to), Ok(cc)) => (to, cc),
                (Err(reason), _) | (_, Err(reason)) => return invalid(reason),
            };
            let id = MailId::new(entry_id().map_err(ToolFault::new)?);
            let mail = NewMail {
                id,
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
                Ok(()) => text(format!("the mail {id} was sent"), false),
                Err(InboxError::MailRefused(reason)) => {
                    text(format!("the mail was refused: {reason}"), true)
                }
                Err(InboxError::Fault(fault)) => Err(ToolFault::new(fault)),
            }
        })
    }
}

struct GetMailFiles {
    inbox: Arc<dyn InboxPort>,
    access: Arc<dyn ThreadAccess>,
    context: InboxContext,
}

fn document_text(document: &MailDocument) -> String {
    match std::str::from_utf8(&document.content) {
        Ok(content) => format!("{}:\n{content}", document.name),
        Err(_) => format!(
            "{}: a document of {} bytes that is not text",
            document.name,
            document.content.len()
        ),
    }
}

impl ToolCode for GetMailFiles {
    fn call<'a>(
        &'a self,
        arguments: &'a OpaqueJson,
        _cancel: CancelView,
        _result: &'a mut dyn ResultStream,
    ) -> BoxFuture<'a, Result<ToolContent, ToolFault>> {
        Box::pin(async move {
            let parsed: FilesArguments = match arguments.deserialize_into() {
                Ok(parsed) => parsed,
                Err(error) => return invalid(error),
            };
            let Ok(entry) = EntryId::parse(&parsed.mail_id) else {
                return invalid(format!("{} is not a mail identity", parsed.mail_id));
            };
            let projected = self
                .access
                .load(&self.context.thread, self.context.agent)
                .await
                .map_err(ToolFault::new)?;
            if projected.entry(&entry).is_err() {
                return text(
                    format!("the mail {entry} is not in your thread: you cannot get its files"),
                    true,
                );
            }
            let documents = self
                .inbox
                .documents(&self.context, MailId::new(entry))
                .await
                .map_err(ToolFault::new)?;
            if documents.is_empty() {
                return text(format!("the mail {entry} has no document"), false);
            }
            let mut blocks = Vec::new();
            for document in &documents {
                let block = block_id().map_err(ToolFault::new)?;
                let text =
                    TextBlock::new(block, document_text(document)).map_err(ToolFault::new)?;
                blocks.push(PlainBlock::Text(text));
            }
            Ok(ToolContent::new(blocks, false))
        })
    }
}
