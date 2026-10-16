//! The test store's operations: each write repeated safely by its rule,
//! the events of a step folded into its turn, a failed or cancelled turn
//! repaired.

use br_llm_messages::{
    Author, StepDraft, StreamEvent, ToolResult, Turn, TurnId, TurnItem, UserInput,
};

use crate::origin::Origin;
use crate::react::{MessageStatus, ThreadFuture, ThreadStore, ThreadView, TurnEnd};
use crate::testkit::faults::Op;
use crate::testkit::repair::repair;
use crate::testkit::store::{
    Draft, Inner, Message, StoreError, TestStore, Thread, TurnRecord, refused, view,
};
use crate::value::ThreadId;

fn thread_mut<'a>(inner: &'a mut Inner, id: &ThreadId) -> Result<&'a mut Thread, StoreError> {
    inner
        .threads
        .get_mut(id)
        .ok_or_else(|| StoreError::Absent(id.clone()))
}

fn turn_mut<'a>(thread: &'a mut Thread, id: &TurnId) -> Option<&'a mut TurnRecord> {
    thread
        .messages
        .iter_mut()
        .find_map(|message| match message {
            Message::Turn(record) if &record.id == id => Some(record.as_mut()),
            Message::Turn(_) | Message::Input(_) => None,
        })
}

fn open_turn_mut<'a>(
    thread: &'a mut Thread,
    id: &TurnId,
) -> Result<&'a mut TurnRecord, StoreError> {
    match turn_mut(thread, id) {
        Some(record) if record.status == MessageStatus::Streaming => Ok(record),
        Some(_) | None => Err(StoreError::NotOpen(id.clone())),
    }
}

fn holds(thread: &Thread, id: &str) -> bool {
    thread.messages.iter().any(|message| match message {
        Message::Input(input) => input.id().as_str() == id,
        Message::Turn(record) => record.id.as_str() == id,
    })
}

/// Folds `event` into the step under way of `record`; at the finish, the
/// step joins the turn.
fn fold(record: &mut TurnRecord, event: &StreamEvent) -> Result<(), StoreError> {
    let draft = record.draft.get_or_insert_with(|| Draft {
        fold: StepDraft::new(),
        events: Vec::new(),
    });
    draft.fold.apply(event.clone()).map_err(refused)?;
    draft.events.push(event.clone());
    if !matches!(event, StreamEvent::Finish { .. }) {
        return Ok(());
    }
    let Some(draft) = record.draft.take() else {
        return Ok(());
    };
    let step = draft.fold.finish(None).map_err(refused)?;
    match &mut record.turn {
        Some(turn) => turn.push_step(step).map_err(refused),
        None => {
            record.turn = Some(Turn::new(
                record.id.clone(),
                Some(record.author.clone()),
                step,
            ));
            Ok(())
        }
    }
}

/// The items `record` holds.
fn items(record: &TurnRecord) -> &[TurnItem] {
    record.turn.as_ref().map_or(&[], Turn::items)
}

impl ThreadStore for TestStore {
    fn read<'a>(&'a self, thread: &'a ThreadId) -> ThreadFuture<'a, Option<ThreadView>> {
        let result = self.write(Op::Read, format!("read {thread}"), |inner| {
            inner.threads.get(thread).map(view).transpose()
        });
        Box::pin(async move { result })
    }

    fn create<'a>(&'a self, thread: &'a ThreadId, origin: &'a Origin) -> ThreadFuture<'a, ()> {
        let result = self.write(Op::Create, format!("create {thread}"), |inner| match inner
            .threads
            .get(thread)
        {
            Some(held) if &held.origin == origin => Ok(()),
            Some(_) => Err(StoreError::Conflict(format!(
                "thread {thread} was created for another origin"
            ))),
            None => {
                inner.threads.insert(
                    thread.clone(),
                    Thread {
                        origin: origin.clone(),
                        messages: Vec::new(),
                    },
                );
                Ok(())
            }
        });
        Box::pin(async move { result })
    }

    fn store_input<'a>(
        &'a self,
        thread: &'a ThreadId,
        input: &'a UserInput,
    ) -> ThreadFuture<'a, ()> {
        let line = format!("input {}", input.id());
        let result = self.write(Op::StoreInput, line, |inner| {
            let held = thread_mut(inner, thread)?;
            let same = held.messages.iter().find_map(|message| match message {
                Message::Input(stored) if stored.id() == input.id() => Some(stored == input),
                Message::Input(_) | Message::Turn(_) => None,
            });
            match same {
                Some(true) => Ok(()),
                Some(false) => Err(StoreError::Conflict(format!(
                    "input {} is held with another content",
                    input.id()
                ))),
                None if holds(held, input.id().as_str()) => Err(StoreError::Conflict(format!(
                    "{} is the id of a turn",
                    input.id()
                ))),
                None => {
                    held.messages.push(Message::Input(input.clone()));
                    Ok(())
                }
            }
        });
        Box::pin(async move { result })
    }

    fn open_turn<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        author: &'a Author,
    ) -> ThreadFuture<'a, ()> {
        let result = self.write(Op::OpenTurn, format!("open {turn}"), |inner| {
            let held = thread_mut(inner, thread)?;
            if let Some(record) = turn_mut(held, turn) {
                return if &record.author == author {
                    Ok(())
                } else {
                    Err(StoreError::Conflict(format!(
                        "turn {turn} is held for another author"
                    )))
                };
            }
            if holds(held, turn.as_str()) {
                return Err(StoreError::Conflict(format!(
                    "{turn} is the id of an input"
                )));
            }
            held.messages.push(Message::Turn(Box::new(TurnRecord {
                id: turn.clone(),
                author: author.clone(),
                status: MessageStatus::Streaming,
                turn: None,
                draft: None,
            })));
            Ok(())
        });
        Box::pin(async move { result })
    }

    fn push_event<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        event: &'a StreamEvent,
    ) -> ThreadFuture<'a, ()> {
        let result = self.write(Op::PushEvent, format!("event {turn}"), |inner| {
            fold(open_turn_mut(thread_mut(inner, thread)?, turn)?, event)
        });
        Box::pin(async move { result })
    }

    fn push_results<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        index: usize,
        results: &'a [ToolResult],
    ) -> ThreadFuture<'a, ()> {
        let line = format!("results {turn}@{index}");
        let result = self.write(Op::PushResults, line, |inner| {
            let record = open_turn_mut(thread_mut(inner, thread)?, turn)?;
            let held = items(record);
            match held.get(index) {
                Some(TurnItem::ToolResults(_)) => return Ok(()),
                Some(TurnItem::Step(_)) => {
                    return Err(StoreError::Conflict(format!(
                        "item {index} of turn {turn} is a step"
                    )));
                }
                None if index != held.len() || record.draft.is_some() => {
                    return Err(StoreError::Refused(format!(
                        "turn {turn} takes its next item at {}",
                        held.len()
                    )));
                }
                None => {}
            }
            if results.is_empty() {
                return Err(StoreError::Refused("an empty tool results item".to_owned()));
            }
            let Some(held) = &mut record.turn else {
                return Err(StoreError::Refused(format!("turn {turn} has no step")));
            };
            held.push_results(results.iter().cloned()).map_err(refused)
        });
        Box::pin(async move { result })
    }

    fn end_turn<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        end: TurnEnd,
    ) -> ThreadFuture<'a, ()> {
        let line = format!("end {turn} {end:?}");
        let result = self.write(Op::EndTurn, line, |inner| {
            let held = thread_mut(inner, thread)?;
            let record = turn_mut(held, turn).ok_or_else(|| StoreError::NotOpen(turn.clone()))?;
            match record.status {
                MessageStatus::Streaming => {}
                status if status == end.status() => return Ok(()),
                status => {
                    return Err(StoreError::Conflict(format!(
                        "turn {turn} is already {status:?}"
                    )));
                }
            }
            match end {
                TurnEnd::Completed if record.draft.is_some() => {
                    return Err(StoreError::Refused(format!(
                        "turn {turn} has a step under way"
                    )));
                }
                TurnEnd::Completed => {}
                TurnEnd::Failed | TurnEnd::Cancelled => repair(record),
            }
            record.status = end.status();
            Ok(())
        });
        Box::pin(async move { result })
    }
}
