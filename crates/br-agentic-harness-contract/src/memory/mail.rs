use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, PoisonError};

use br_agentic_contract::memory::MemoryIncoming;
use br_agentic_contract::{
    Cursor, DocumentRef, IncomingBatch, IncomingContext, IncomingPort, MailDocument, MailId,
    NewMail, PortError, PortFuture,
};
use br_llm_messages::EntryId;

use super::record::{HarnessCall, HarnessRecord};
use crate::mail::MailPort;

#[derive(Clone)]
pub struct MemoryMail {
    record: HarnessRecord,
    incoming: MemoryIncoming,
    documents: Arc<Mutex<HashMap<DocumentRef, Vec<u8>>>>,
}

impl MemoryMail {
    pub fn new(record: &HarnessRecord, incoming: MemoryIncoming) -> Self {
        Self {
            record: record.clone(),
            incoming,
            documents: Arc::default(),
        }
    }

    pub fn incoming(&self) -> &MemoryIncoming {
        &self.incoming
    }

    pub fn put_document(&self, reference: DocumentRef, content: Vec<u8>) {
        self.documents
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(reference, content);
    }
}

impl MailPort for MemoryMail {
    fn items<'a>(
        &'a self,
        context: &'a IncomingContext,
        after: Option<&'a Cursor>,
        limit: NonZeroUsize,
    ) -> PortFuture<'a, Result<IncomingBatch, PortError>> {
        self.record.push(HarnessCall::MailItems {
            context: *context,
            after: after.cloned(),
            limit,
        });
        self.incoming.items(context, after, limit)
    }

    fn mark_read<'a>(
        &'a self,
        context: &'a IncomingContext,
        item: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(HarnessCall::MailMarkRead {
            context: *context,
            item,
        });
        self.incoming.mark_read(context, item)
    }

    fn send<'a>(
        &'a self,
        context: &'a IncomingContext,
        mail: NewMail,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(HarnessCall::MailSend {
            context: *context,
            mail: mail.id,
        });
        Box::pin(async move { self.incoming.deliver_mail(context.agent, mail) })
    }

    fn documents<'a>(
        &'a self,
        context: &'a IncomingContext,
        mail: MailId,
    ) -> PortFuture<'a, Result<Vec<MailDocument>, PortError>> {
        self.record.push(HarnessCall::MailDocuments {
            context: *context,
            mail,
        });
        Box::pin(async move {
            let sent = self
                .incoming
                .mail(mail)
                .filter(|sent| sent.mail.to.contains(&context.agent))
                .ok_or_else(|| {
                    PortError::refused("the agent is not a recipient in to of that mail")
                })?;
            let documents = self
                .documents
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            sent.mail
                .attachments
                .iter()
                .map(|attachment| {
                    documents
                        .get(attachment.document())
                        .map(|content| MailDocument {
                            name: attachment.name().to_owned(),
                            content: content.clone(),
                        })
                        .ok_or_else(|| PortError::fault("a document of the mail is unavailable"))
                })
                .collect()
        })
    }
}
