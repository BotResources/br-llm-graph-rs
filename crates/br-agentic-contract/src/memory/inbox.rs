use std::num::NonZeroUsize;

use br_llm_clients_core::BoxFuture;
use tokio::sync::broadcast::error::RecvError;

use super::state::{MemoryHub, State, StoredMail};
use crate::fault::{InboxError, PortFault};
use crate::ids::MailId;
use crate::inbox::{Cursor, InboxBatch, InboxContext, InboxItem, InboxPort};
use crate::mail::{MailDocument, MailItem, NewMail};

#[derive(Clone)]
pub struct MemoryInbox {
    hub: MemoryHub,
}

impl MemoryHub {
    pub fn inbox(&self) -> MemoryInbox {
        MemoryInbox { hub: self.clone() }
    }
}

fn mail_item(stored: &StoredMail, context: &InboxContext) -> Option<MailItem> {
    if stored.read.contains(&context.agent) {
        return None;
    }
    if stored.to.contains(&context.agent) {
        return Some(MailItem::To {
            id: stored.id,
            sender: stored.sender,
            to: stored.to.clone(),
            cc: stored.cc.clone(),
            subject: stored.subject.clone(),
            body: stored.body.clone(),
            attachment_names: stored
                .attachments
                .iter()
                .map(|attachment| attachment.name().to_owned())
                .collect(),
        });
    }
    stored.cc.contains(&context.agent).then(|| MailItem::Cc {
        id: stored.id,
        subject: stored.subject.clone(),
    })
}

fn listed(state: &State, context: &InboxContext) -> Result<Vec<(u64, InboxItem)>, PortFault> {
    let conversation = state
        .threads
        .get(&context.thread)
        .ok_or_else(|| PortFault::new(format!("unknown thread {}", context.thread)))?;
    let projected = conversation.project_for(context.agent);
    let mut items: Vec<(u64, InboxItem)> = state
        .mails
        .iter()
        .filter_map(|stored| {
            mail_item(stored, context).map(|item| (stored.seq, InboxItem::Mail(item)))
        })
        .collect();
    items.extend(
        state
            .entries
            .iter()
            .filter(|stored| stored.agent == context.agent)
            .filter(|stored| projected.entry(&stored.entry.id()).is_err())
            .map(|stored| (stored.seq, InboxItem::Entry(stored.entry.clone()))),
    );
    items.sort_by_key(|(seq, _)| *seq);
    Ok(items)
}

fn position(cursor: Option<&Cursor>) -> Result<u64, PortFault> {
    match cursor {
        None => Ok(0),
        Some(cursor) => cursor
            .as_str()
            .parse()
            .map_err(|_| PortFault::new("the cursor was not given by this inbox")),
    }
}

impl InboxPort for MemoryInbox {
    fn items<'a>(
        &'a self,
        context: &'a InboxContext,
        after: Option<&'a Cursor>,
        limit: NonZeroUsize,
    ) -> BoxFuture<'a, Result<InboxBatch, PortFault>> {
        Box::pin(async move {
            let after = position(after)?;
            let all = {
                let mut state = self.hub.lock();
                let all = listed(&state, context)?;
                let given = state.given.entry(context.agent).or_insert(0);
                let reached = all
                    .iter()
                    .filter(|(seq, _)| *seq > after)
                    .take(limit.get())
                    .map(|(seq, _)| *seq)
                    .max()
                    .unwrap_or(0);
                *given = (*given).max(reached);
                all
            };
            let mut rest = all.into_iter().filter(|(seq, _)| *seq > after);
            let page: Vec<(u64, InboxItem)> = rest.by_ref().take(limit.get()).collect();
            let more = rest.next().is_some();
            let next = match page.last() {
                Some((seq, _)) if more => Some(Cursor::new(seq.to_string())),
                _ => None,
            };
            Ok(InboxBatch {
                items: page.into_iter().map(|(_, item)| item).collect(),
                next,
            })
        })
    }

    fn wait<'a>(
        &'a self,
        context: &'a InboxContext,
    ) -> BoxFuture<'a, Result<InboxItem, PortFault>> {
        let mut arrivals = self.hub.arrivals.subscribe();
        Box::pin(async move {
            loop {
                let found = {
                    let mut state = self.hub.lock();
                    let floor = state.given.get(&context.agent).copied().unwrap_or(0);
                    let found = listed(&state, context)?
                        .into_iter()
                        .find(|(seq, _)| *seq > floor);
                    if let Some((seq, _)) = &found {
                        state.given.insert(context.agent, *seq);
                    }
                    found
                };
                if let Some((_, item)) = found {
                    return Ok(item);
                }
                match arrivals.recv().await {
                    Ok(agent) if agent != context.agent => {}
                    Ok(_) | Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => {
                        return Err(PortFault::new("the inbox was closed"));
                    }
                }
            }
        })
    }

    fn mark_read<'a>(
        &'a self,
        context: &'a InboxContext,
        mail: MailId,
    ) -> BoxFuture<'a, Result<(), PortFault>> {
        Box::pin(async move {
            if let Some(stored) = self.hub.lock().mails.iter_mut().find(|m| m.id == mail) {
                stored.read.insert(context.agent);
            }
            Ok(())
        })
    }

    fn send<'a>(
        &'a self,
        context: &'a InboxContext,
        mail: NewMail,
    ) -> BoxFuture<'a, Result<(), InboxError>> {
        Box::pin(async move {
            if let Some(reason) = mail.refusal() {
                return Err(InboxError::MailRefused(reason));
            }
            {
                let mut state = self.hub.lock();
                if state.mails.iter().any(|stored| stored.id == mail.id) {
                    return Ok(());
                }
                let seq = state.take_seq();
                state.mails.push(StoredMail {
                    seq,
                    id: mail.id,
                    sender: context.agent,
                    to: mail.to.clone(),
                    cc: mail.cc.clone(),
                    subject: mail.subject,
                    body: mail.body,
                    attachments: mail.attachments,
                    read: Default::default(),
                });
            }
            for recipient in mail.to.iter().chain(mail.cc.iter()) {
                self.hub.announce(*recipient);
            }
            Ok(())
        })
    }

    fn documents<'a>(
        &'a self,
        context: &'a InboxContext,
        mail: MailId,
    ) -> BoxFuture<'a, Result<Vec<MailDocument>, PortFault>> {
        Box::pin(async move {
            let state = self.hub.lock();
            let Some(stored) = state.mails.iter().find(|stored| stored.id == mail) else {
                return Ok(Vec::new());
            };
            let visible = stored.sender == context.agent
                || stored.to.contains(&context.agent)
                || stored.cc.contains(&context.agent);
            if !visible {
                return Ok(Vec::new());
            }
            stored
                .attachments
                .iter()
                .map(|attachment| {
                    state
                        .documents
                        .get(attachment.document())
                        .map(|content| MailDocument {
                            name: attachment.name().to_owned(),
                            content: content.clone(),
                        })
                        .ok_or_else(|| PortFault::new("a document of the mail is unavailable"))
                })
                .collect()
        })
    }
}
