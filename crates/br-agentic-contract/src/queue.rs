use br_llm_messages::{EntryId, ProjectedThread, Turn, TurnId};

use crate::fault::{PortFault, ThreadError};
use crate::inbox::{InboxContext, InboxItem, InboxPort};
use crate::thread_access::{Incoming, ThreadAccess, TurnTarget};

#[derive(Debug, Default)]
pub struct Queue {
    items: Vec<InboxItem>,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, item: InboxItem) -> bool {
        if self.items.iter().any(|held| held.id() == item.id()) {
            return false;
        }
        self.items.push(item);
        true
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn items(&self) -> &[InboxItem] {
        &self.items
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Boundary {
    pub stored: Vec<EntryId>,
    pub skipped: Vec<EntryId>,
}

#[derive(Debug, thiserror::Error)]
pub enum BoundaryError {
    #[error(transparent)]
    Thread(#[from] ThreadError),
    #[error(transparent)]
    Port(#[from] PortFault),
}

enum Placement {
    Open(TurnTarget),
    NewInput(TurnId),
}

fn placement_of(projected: &ProjectedThread, new_turn: TurnId) -> Placement {
    let own_open = projected
        .turns()
        .iter()
        .rev()
        .find_map(Turn::as_ai)
        .filter(|ai| !ai.is_closed());
    if let Some(ai) = own_open {
        return Placement::Open(TurnTarget::Ai {
            id: ai.id(),
            author: ai.author().clone(),
        });
    }
    match projected.turns().last() {
        Some(Turn::InputTurn(input)) => Placement::Open(TurnTarget::Input(input.id())),
        _ => Placement::NewInput(new_turn),
    }
}

pub async fn store_at_boundary(
    access: &dyn ThreadAccess,
    inbox: &dyn InboxPort,
    context: &InboxContext,
    queue: &mut Queue,
    new_turn: TurnId,
) -> Result<Boundary, BoundaryError> {
    let projected = access.load(&context.thread, context.agent).await?;
    let mut placement = placement_of(&projected, new_turn);
    let mut report = Boundary::default();
    let mut pending = std::mem::take(&mut queue.items).into_iter();
    while let Some(item) = pending.next() {
        let outcome = store_one(
            access,
            inbox,
            context,
            &projected,
            &mut placement,
            &item,
            &mut report,
        )
        .await;
        if let Err(error) = outcome {
            queue.items.push(item);
            queue.items.extend(pending);
            return Err(error);
        }
    }
    Ok(report)
}

async fn store_one(
    access: &dyn ThreadAccess,
    inbox: &dyn InboxPort,
    context: &InboxContext,
    projected: &ProjectedThread,
    placement: &mut Placement,
    item: &InboxItem,
    report: &mut Boundary,
) -> Result<(), BoundaryError> {
    let id = item.id();
    if projected.entry(&id).is_ok() {
        mark_if_mail(inbox, context, item).await?;
        report.skipped.push(id);
        return Ok(());
    }
    let incoming = match item {
        InboxItem::Mail(mail) => Incoming::Mail(mail.clone()),
        InboxItem::Entry(entry) => Incoming::Entry(entry.clone()),
    };
    match placement {
        Placement::NewInput(turn) => {
            let turn = *turn;
            access
                .add_input_turn(&context.thread, context.agent, turn, vec![incoming])
                .await?;
            *placement = Placement::Open(TurnTarget::Input(turn));
        }
        Placement::Open(target) => {
            access
                .add_incoming(&context.thread, context.agent, target.clone(), incoming)
                .await?;
        }
    }
    mark_if_mail(inbox, context, item).await?;
    report.stored.push(id);
    Ok(())
}

async fn mark_if_mail(
    inbox: &dyn InboxPort,
    context: &InboxContext,
    item: &InboxItem,
) -> Result<(), PortFault> {
    match item {
        InboxItem::Mail(mail) => inbox.mark_read(context, mail.id()).await,
        InboxItem::Entry(_) => Ok(()),
    }
}
