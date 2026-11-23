use std::collections::HashSet;

use br_llm_messages::{EntryId, Error, IncomingEntry, InputTurn, TurnEntry, TurnId};

use br_agentic_contract::{AiTurnRef, IncomingContext, IncomingItem, IncomingPort, MailItem};

use crate::queue::Queue;
use crate::session::Session;
use crate::session_error::SessionError;

pub type MailEntryBuilder<'a> = &'a (dyn Fn(&MailItem) -> Result<IncomingEntry, Error> + Sync);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Boundary {
    pub stored: Vec<EntryId>,
    pub skipped: Vec<EntryId>,
    pub unmarked: Vec<EntryId>,
}

#[derive(Debug, thiserror::Error)]
#[error("the instance faulted at the boundary")]
#[non_exhaustive]
pub struct BoundaryError {
    #[source]
    pub fault: SessionError,
    pub done: Boundary,
}

struct Run<'a, 'b> {
    session: &'a mut Session<'b>,
    incoming: &'a dyn IncomingPort,
    context: &'a IncomingContext,
    done: HashSet<EntryId>,
    report: Boundary,
}

impl Run<'_, '_> {
    async fn mark(&mut self, item: &IncomingItem) {
        self.done.insert(item.id());
        if self
            .incoming
            .mark_read(self.context, item.id())
            .await
            .is_err()
        {
            self.report.unmarked.push(item.id());
        }
    }

    async fn skip_held(&mut self, held: &[IncomingItem]) {
        for item in held {
            self.report.skipped.push(item.id());
            self.mark(item).await;
        }
    }

    async fn store_in_open_turn(
        &mut self,
        turn: &AiTurnRef,
        fresh: &[(IncomingItem, IncomingEntry)],
    ) -> Result<(), SessionError> {
        for (item, entry) in fresh {
            let entry = TurnEntry::try_from(entry.clone())?;
            self.session.add_entry(turn, entry).await?;
            self.report.stored.push(item.id());
            self.mark(item).await;
        }
        Ok(())
    }

    async fn store_in_new_input_turn(
        &mut self,
        turn: TurnId,
        fresh: &[(IncomingItem, IncomingEntry)],
    ) -> Result<(), SessionError> {
        if fresh.is_empty() {
            return Ok(());
        }
        let entries = fresh.iter().map(|(_, entry)| entry.clone()).collect();
        self.session
            .add_input_turn(InputTurn::new(turn, entries)?)
            .await?;
        for (item, _) in fresh {
            self.report.stored.push(item.id());
        }
        for (item, _) in fresh {
            self.mark(item).await;
        }
        Ok(())
    }
}

fn is_entry_of(mail: &MailItem, entry: &IncomingEntry) -> bool {
    entry.id() == mail.id().entry_id()
        && match (mail, entry) {
            (
                MailItem::To {
                    sender,
                    author,
                    source,
                    ..
                },
                IncomingEntry::AgentMail(agent_mail),
            ) => {
                agent_mail.sender() == *sender
                    && agent_mail.author() == author
                    && agent_mail.source_conversation() == *source
            }
            (MailItem::Cc { .. }, IncomingEntry::RuntimeNotice(_)) => true,
            _ => false,
        }
}

fn build(
    item: &IncomingItem,
    mail_entry: MailEntryBuilder<'_>,
) -> Result<IncomingEntry, SessionError> {
    match item {
        IncomingItem::Mail(mail) => {
            let entry = mail_entry(mail)?;
            if is_entry_of(mail, &entry) {
                Ok(entry)
            } else {
                Err(SessionError::WrongMailEntry(mail.id()))
            }
        }
        IncomingItem::Entry(entry) => Ok(entry.entry().clone()),
    }
}

pub async fn store_at_boundary(
    session: &mut Session<'_>,
    incoming: &dyn IncomingPort,
    context: &IncomingContext,
    queue: &mut Queue,
    mail_entry: MailEntryBuilder<'_>,
    new_turn: TurnId,
) -> Result<Boundary, BoundaryError> {
    let pending = queue.take();
    let mut run = Run {
        session,
        incoming,
        context,
        done: HashSet::new(),
        report: Boundary::default(),
    };
    let outcome = run_boundary(&mut run, &pending, mail_entry, new_turn).await;
    let Run { done, report, .. } = run;
    match outcome {
        Ok(()) => Ok(report),
        Err(fault) => {
            queue.restore(
                pending
                    .into_iter()
                    .filter(|item| !done.contains(&item.id()))
                    .collect(),
            );
            Err(BoundaryError {
                fault,
                done: report,
            })
        }
    }
}

async fn run_boundary(
    run: &mut Run<'_, '_>,
    pending: &[IncomingItem],
    mail_entry: MailEntryBuilder<'_>,
    new_turn: TurnId,
) -> Result<(), SessionError> {
    let mut held = Vec::new();
    let mut fresh = Vec::new();
    for item in pending {
        if run.session.holds(item.id()) {
            held.push(item.clone());
        } else {
            fresh.push((item.clone(), build(item, mail_entry)?));
        }
    }
    run.skip_held(&held).await;
    match run.session.open_ai_turn(run.context.agent) {
        Some(turn) => run.store_in_open_turn(&turn, &fresh).await,
        None => run.store_in_new_input_turn(new_turn, &fresh).await,
    }
}
