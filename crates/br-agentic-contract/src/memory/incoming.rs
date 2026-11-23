use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::record::{Call, Record};
use crate::entry_item::EntryItem;
use crate::error::PortError;
use crate::future::PortFuture;
use crate::ids::MailId;
use crate::incoming::{Cursor, IncomingBatch, IncomingContext, IncomingItem, IncomingPort};
use crate::mail::{MailItem, NewMail};
use br_llm_messages::{AgentId, EntryId};

#[derive(Debug, Clone)]
pub struct SentMail {
    pub sender: AgentId,
    pub mail: NewMail,
}

struct Listed {
    seq: u64,
    agent: AgentId,
    item: IncomingItem,
}

#[derive(Default)]
struct State {
    next_seq: u64,
    mails: Vec<(u64, SentMail)>,
    entries: Vec<(u64, AgentId, EntryItem)>,
    read: HashSet<(AgentId, EntryId)>,
}

struct Shared {
    state: Mutex<State>,
}

#[derive(Clone)]
pub struct MemoryIncoming {
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
                        item: IncomingItem::Mail(item),
                    });
                }
            }
        }
        for (seq, agent, entry) in &self.entries {
            listed.push(Listed {
                seq: *seq,
                agent: *agent,
                item: IncomingItem::Entry(entry.clone()),
            });
        }
        listed.sort_by_key(|listed| listed.seq);
        listed
    }

    fn unread(&self, agent: AgentId) -> Vec<(u64, IncomingItem)> {
        self.listed()
            .into_iter()
            .filter(|listed| listed.agent == agent)
            .filter(|listed| !self.read.contains(&(agent, listed.item.id())))
            .map(|listed| (listed.seq, listed.item))
            .collect()
    }
}

impl MemoryIncoming {
    pub fn new(record: &Record) -> Self {
        Self {
            record: record.clone(),
            shared: Arc::new(Shared {
                state: Mutex::new(State::default()),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn deliver_entry(&self, agent: AgentId, entry: EntryItem) {
        let mut state = self.lock();
        let seq = state.take_seq();
        state.entries.push((seq, agent, entry));
    }

    pub fn deliver_mail(&self, sender: AgentId, mail: NewMail) -> Result<(), PortError> {
        if let Some(reason) = mail.refusal() {
            return Err(PortError::Refused(reason));
        }
        let mut state = self.lock();
        if state.mails.iter().any(|(_, sent)| sent.mail.id == mail.id) {
            return Ok(());
        }
        let seq = state.take_seq();
        state.mails.push((seq, SentMail { sender, mail }));
        Ok(())
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
            .map_err(|_| PortError::fault("the cursor was not given by this incoming port")),
    }
}

impl IncomingPort for MemoryIncoming {
    fn items<'a>(
        &'a self,
        context: &'a IncomingContext,
        after: Option<&'a Cursor>,
        limit: NonZeroUsize,
    ) -> PortFuture<'a, Result<IncomingBatch, PortError>> {
        self.record.push(Call::IncomingItems {
            context: *context,
            after: after.cloned(),
            limit,
        });
        Box::pin(async move {
            let after = position(after)?;
            let unread = self.lock().unread(context.agent);
            let mut rest = unread.into_iter().filter(|(seq, _)| *seq > after);
            let page: Vec<(u64, IncomingItem)> = rest.by_ref().take(limit.get()).collect();
            let next = match page.last() {
                Some((seq, _)) if rest.next().is_some() => Some(Cursor::new(seq.to_string())),
                _ => None,
            };
            Ok(IncomingBatch {
                items: page.into_iter().map(|(_, item)| item).collect(),
                next,
            })
        })
    }

    fn mark_read<'a>(
        &'a self,
        context: &'a IncomingContext,
        item: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(Call::IncomingMarkRead {
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
}
