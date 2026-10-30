use std::collections::HashSet;

use br_llm_messages::{EntryId, Error, IncomingEntry, InputTurn, TurnEntry, TurnId};

use crate::inbox::{InboxContext, InboxItem, InboxPort};
use crate::mail::MailItem;
use crate::queue::Queue;
use crate::rules::SessionError;
use crate::session::Session;
use crate::thread_access::AiTurnRef;

pub type MailEntryBuilder<'a> = &'a (dyn Fn(&MailItem) -> Result<IncomingEntry, Error> + Sync);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Boundary {
    pub stored: Vec<EntryId>,
    pub skipped: Vec<EntryId>,
}

#[derive(Debug, thiserror::Error)]
#[error("{fault}")]
pub struct BoundaryError {
    pub fault: SessionError,
    pub done: Boundary,
}

struct Run<'a, 'b> {
    session: &'a mut Session<'b>,
    inbox: &'a dyn InboxPort,
    context: &'a InboxContext,
    done: HashSet<EntryId>,
    report: Boundary,
}

impl Run<'_, '_> {
    async fn mark(&mut self, item: &InboxItem) -> Result<(), SessionError> {
        self.inbox.mark_read(self.context, item.id()).await?;
        self.done.insert(item.id());
        Ok(())
    }

    async fn skip_held(&mut self, held: &[InboxItem]) -> Result<(), SessionError> {
        for item in held {
            self.mark(item).await?;
            self.report.skipped.push(item.id());
        }
        Ok(())
    }

    async fn store_in_open_turn(
        &mut self,
        turn: &AiTurnRef,
        fresh: &[(InboxItem, IncomingEntry)],
    ) -> Result<(), SessionError> {
        for (item, entry) in fresh {
            let entry = TurnEntry::try_from(entry.clone())?;
            self.session.add_entry(turn, entry).await?;
            self.report.stored.push(item.id());
            self.mark(item).await?;
        }
        Ok(())
    }

    async fn store_in_new_input_turn(
        &mut self,
        turn: TurnId,
        fresh: &[(InboxItem, IncomingEntry)],
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
            self.mark(item).await?;
        }
        Ok(())
    }
}

fn build(
    item: &InboxItem,
    mail_entry: MailEntryBuilder<'_>,
) -> Result<IncomingEntry, SessionError> {
    match item {
        InboxItem::Mail(mail) => Ok(mail_entry(mail)?),
        InboxItem::Entry(entry) => Ok(entry.entry().clone()),
    }
}

pub async fn store_at_boundary(
    session: &mut Session<'_>,
    inbox: &dyn InboxPort,
    context: &InboxContext,
    queue: &mut Queue,
    mail_entry: MailEntryBuilder<'_>,
    new_turn: TurnId,
) -> Result<Boundary, BoundaryError> {
    let pending = queue.take();
    let mut run = Run {
        session,
        inbox,
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
    pending: &[InboxItem],
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
    run.skip_held(&held).await?;
    match run.session.open_ai_turn(run.context.agent) {
        Some(turn) => run.store_in_open_turn(&turn, &fresh).await,
        None => run.store_in_new_input_turn(new_turn, &fresh).await,
    }
}
