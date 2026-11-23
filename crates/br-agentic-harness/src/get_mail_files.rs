use std::sync::Arc;

use br_agentic_contract::{
    IncomingContext, MailDocument, MailId, PortFuture, ResultStream, ToolBlock, ToolCode,
    ToolContent, ToolFault,
};
use br_agentic_harness_contract::{FilePath, MailPort, SandboxPort};
use br_graph_contract::CancelView;
use br_llm_messages::{EntryId, OpaqueJson};
use serde::Deserialize;

use crate::shared::Shared;
use crate::tool_text::{error, invalid, quote};

#[derive(Deserialize)]
struct FilesArguments {
    mail_id: String,
}

pub(crate) struct GetMailFiles {
    pub(crate) mail: Arc<dyn MailPort>,
    pub(crate) sandbox: Option<Arc<dyn SandboxPort>>,
    pub(crate) context: IncomingContext,
    pub(crate) shared: Arc<Shared>,
}

fn document_block(document: &MailDocument) -> Result<ToolBlock, ToolFault> {
    let text = match std::str::from_utf8(&document.content) {
        Ok(content) => format!("{}:\n{content}", quote(&document.name)),
        Err(_) => format!(
            "{}: a document of {} bytes that is not text",
            quote(&document.name),
            document.content.len()
        ),
    };
    ToolBlock::text(text).map_err(ToolFault::new)
}

fn placeable(name: &str) -> bool {
    !matches!(name, "" | "." | "..") && !name.contains(['/', '\\'])
}

impl GetMailFiles {
    async fn copy_to_work_folder(&self, mail: MailId, documents: &[MailDocument]) -> Vec<String> {
        let (Some(sandbox), Some(boxed)) =
            (&self.sandbox, self.shared.runs.boxed(self.context.instance))
        else {
            return Vec::new();
        };
        let mut left = Vec::new();
        for document in documents {
            let placed = match placeable(&document.name)
                .then(|| FilePath::new(&format!("mail/{mail}/{}", document.name)))
            {
                Some(Ok(path)) => sandbox
                    .write(&boxed, &path, document.content.clone())
                    .await
                    .is_ok(),
                _ => false,
            };
            if !placed {
                left.push(document.name.clone());
            }
        }
        left
    }
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
            if !self.shared.mails.holds(&self.context.thread, entry) {
                return error(format!(
                    "the mail {entry} is not in your conversation: you cannot get its files"
                ));
            }
            let mail = MailId::new(entry);
            let documents = match self.mail.documents(&self.context, mail).await {
                Ok(documents) => documents,
                Err(failure) => {
                    return match failure.reason() {
                        Some(reason) => error(format!("the inbox refused the request: {reason}")),
                        None => error("the inbox could not do it now"),
                    };
                }
            };
            let mut blocks = documents
                .iter()
                .map(document_block)
                .collect::<Result<Vec<_>, _>>()?;
            let left = self.copy_to_work_folder(mail, &documents).await;
            if !left.is_empty() {
                let names = left
                    .iter()
                    .map(|name| quote(name))
                    .collect::<Vec<_>>()
                    .join(", ");
                blocks.push(
                    ToolBlock::text(format!(
                        "these documents were not placed in the work folder: {names}"
                    ))
                    .map_err(ToolFault::new)?,
                );
            }
            Ok(ToolContent::new(blocks, false))
        })
    }
}
