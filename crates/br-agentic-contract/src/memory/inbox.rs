use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use br_llm_messages::{AgentId, EntryId};
use tokio::sync::watch;

use super::record::{Call, Record};
use crate::error::PortError;
use crate::future::PortFuture;
use crate::ids::MailId;
use crate::inbox::{Cursor, InboxBatch, InboxContext, InboxItem, InboxPort};
use crate::inbox_entry::InboxEntry;
use crate::mail::{DocumentRef, MailDocument, MailItem, NewMail};

#[derive(Debug, Clone)]
pub struct SentMail {
    pub sender: AgentId,
    pub mail: NewMail,
}

struct Listed {
    seq: u64,
    agent: AgentId,
    item: InboxItem,
}

#[derive(Default)]
struct State {
    next_seq: u64,
    mails: Vec<(u64, SentMail)>,
    entries: Vec<(u64, AgentId, InboxEntry)>,
    documents: HashMap<DocumentRef, Vec<u8>>,
    read: HashSet<(AgentId, EntryId)>,
}

struct Shared {
    state: Mutex<State>,
    arrivals: watch::Sender<u64>,
}

#[derive(Clone)]
pub struct MemoryInbox {
    record: Record,
    shared: Arc<Shared>,
}

fn mail_item(sent: &SentMail, agent: AgentId) -> Option<MailItem> {
    let mail = &sent.mail;
    if mail.to.contains(&agent) {
        return Some(MailItem::To {
            id: mail.id,
            sender: sent.sender,
            author: mail.author.clone(),
            source: mail.source,
            to: mail.to.clone(),
            cc: mail.cc.clone(),
            subject: mail.subject.clone(),
            body: mail.body.clone(),
            attachment_names: mail
                .attachments
                .iter()
                .map(|attachment| attachment.name().to_owned())
                .collect(),
        });
    }
    mail.cc.contains(&agent).then(|| MailItem::Cc {
        id: mail.id,
        subject: mail.subject.clone(),
    })
}

impl State {
    fn take_seq(&mut self) -> u64 {
        self.next_seq += 1;
        self.next_seq
    }

    fn listed(&self) -> Vec<Listed> {
        let mut listed = Vec::new();
        for (seq, sent) in &self.mails {
            for agent in sent.mail.to.iter().chain(sent.mail.cc.iter()) {
                if let Some(item) = mail_item(sent, *agent) {
                    listed.push(Listed {
                        seq: *seq,
                        agent: *agent,
                        item: InboxItem::Mail(item),
                    });
                }
            }
        }
        for (seq, agent, entry) in &self.entries {
            listed.push(Listed {
                seq: *seq,
                agent: *agent,
                item: InboxItem::Entry(entry.clone()),
            });
        }
        listed.sort_by_key(|listed| listed.seq);
        listed
    }

    fn unread(&self, agent: AgentId) -> Vec<(u64, InboxItem)> {
        self.listed()
            .into_iter()
            .filter(|listed| listed.agent == agent)
            .filter(|listed| !self.read.contains(&(agent, listed.item.id())))
            .map(|listed| (listed.seq, listed.item))
            .collect()
    }
}

impl MemoryInbox {
    pub fn new(record: &Record) -> Self {
        let (arrivals, _) = watch::channel(0);
        Self {
            record: record.clone(),
            shared: Arc::new(Shared {
                state: Mutex::new(State::default()),
                arrivals,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn announce(&self) {
        self.shared.arrivals.send_modify(|count| *count += 1);
    }

    pub fn deliver_entry(&self, agent: AgentId, entry: InboxEntry) {
        {
            let mut state = self.lock();
            let seq = state.take_seq();
            state.entries.push((seq, agent, entry));
        }
        self.announce();
    }

    pub fn deliver_mail(&self, sender: AgentId, mail: NewMail) -> Result<(), PortError> {
        if let Some(reason) = mail.refusal() {
            return Err(PortError::Refused(reason));
        }
        {
            let mut state = self.lock();
            if state.mails.iter().any(|(_, sent)| sent.mail.id == mail.id) {
                return Ok(());
            }
            let seq = state.take_seq();
            state.mails.push((seq, SentMail { sender, mail }));
        }
        self.announce();
        Ok(())
    }

    pub fn put_document(&self, reference: DocumentRef, content: Vec<u8>) {
        self.lock().documents.insert(reference, content);
    }

    pub fn mail(&self, id: MailId) -> Option<SentMail> {
        self.lock()
            .mails
            .iter()
            .find(|(_, sent)| sent.mail.id == id)
            .map(|(_, sent)| sent.clone())
    }

    pub fn is_read(&self, agent: AgentId, item: EntryId) -> bool {
        self.lock().read.contains(&(agent, item))
    }
}

fn position(cursor: Option<&Cursor>) -> Result<u64, PortError> {
    match cursor {
        None => Ok(0),
        Some(cursor) => cursor
            .as_str()
            .parse()
            .map_err(|_| PortError::fault("the cursor was not given by this inbox")),
    }
}

impl InboxPort for MemoryInbox {
    fn items<'a>(
        &'a self,
        context: &'a InboxContext,
        after: Option<&'a Cursor>,
        limit: NonZeroUsize,
    ) -> PortFuture<'a, Result<InboxBatch, PortError>> {
        self.record.push(Call::InboxItems {
            context: *context,
            after: after.cloned(),
            limit,
        });
        Box::pin(async move {
            let after = position(after)?;
            let unread = self.lock().unread(context.agent);
            let mut rest = unread.into_iter().filter(|(seq, _)| *seq > after);
            let page: Vec<(u64, InboxItem)> = rest.by_ref().take(limit.get()).collect();
            let next = match page.last() {
                Some((seq, _)) if rest.next().is_some() => Some(Cursor::new(seq.to_string())),
                _ => None,
            };
            Ok(InboxBatch {
                items: page.into_iter().map(|(_, item)| item).collect(),
                next,
            })
        })
    }

    fn wait<'a>(&'a self, context: &'a InboxContext) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(Call::InboxWait { context: *context });
        let mut arrivals = self.shared.arrivals.subscribe();
        Box::pin(async move {
            loop {
                if !self.lock().unread(context.agent).is_empty() {
                    return Ok(());
                }
                if arrivals.changed().await.is_err() {
                    return Err(PortError::fault("the inbox was closed"));
                }
            }
        })
    }

    fn mark_read<'a>(
        &'a self,
        context: &'a InboxContext,
        item: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(Call::InboxMarkRead {
            context: *context,
            item,
        });
        Box::pin(async move {
            let mut state = self.lock();
            let held = state
                .listed()
                .iter()
                .any(|listed| listed.agent == context.agent && listed.item.id() == item);
            if held {
                state.read.insert((context.agent, item));
            }
            Ok(())
        })
    }

    fn send<'a>(
        &'a self,
        context: &'a InboxContext,
        mail: NewMail,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(Call::InboxSend {
            context: *context,
            mail: mail.clone(),
        });
        Box::pin(async move { self.deliver_mail(context.agent, mail) })
    }

    fn documents<'a>(
        &'a self,
        context: &'a InboxContext,
        mail: MailId,
    ) -> PortFuture<'a, Result<Vec<MailDocument>, PortError>> {
        self.record.push(Call::InboxDocuments {
            context: *context,
            mail,
        });
        Box::pin(async move {
            let state = self.lock();
            let sent = state
                .mails
                .iter()
                .map(|(_, sent)| sent)
                .find(|sent| sent.mail.id == mail && sent.mail.to.contains(&context.agent))
                .ok_or_else(|| {
                    PortError::refused("the agent is not a recipient in to of that mail")
                })?;
            sent.mail
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
                        .ok_or_else(|| PortError::fault("a document of the mail is unavailable"))
                })
                .collect()
        })
    }
}
