//! A thread store for tests that behaves as a real store: it keeps threads
//! in memory, folds the stream events of each step, repeats each write
//! safely by the rules of `ThreadStore`, repairs a failed or cancelled turn,
//! and fails or acts on chosen operations (see `faults.rs`).

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use br_llm_messages::{
    Author, Conversation, StepDraft, StreamEvent, ToolResult, Turn, TurnId, TurnItem, UserInput,
};

use crate::origin::Origin;
use crate::react::{
    MessageStatus, ThreadError, ThreadFuture, ThreadMessage, ThreadStore, ThreadView, TurnEnd,
};
use crate::testkit::faults::{Faults, Op};
use crate::testkit::repair::repair;
use crate::value::ThreadId;

/// Why the test store refused an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The thread is absent.
    Absent(ThreadId),
    /// The id is held with other content.
    Conflict(String),
    /// The turn is not open, or not in the thread.
    NotOpen(TurnId),
    /// The write breaks a rule of br-llm-messages or of the store.
    Refused(String),
    /// A fault injected by the test.
    Injected(Op),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Absent(thread) => write!(f, "thread {thread} is absent"),
            StoreError::Conflict(what) => write!(f, "conflict: {what}"),
            StoreError::NotOpen(turn) => write!(f, "turn {turn} is not open"),
            StoreError::Refused(why) => write!(f, "refused: {why}"),
            StoreError::Injected(op) => write!(f, "injected fault on {op:?}"),
        }
    }
}

impl std::error::Error for StoreError {}

/// A turn of a thread: the steps and results held so far (`None` before its
/// first step), and the events of the step under way.
pub(crate) struct TurnRecord {
    pub(crate) id: TurnId,
    pub(crate) author: Author,
    pub(crate) status: MessageStatus,
    pub(crate) turn: Option<Turn>,
    pub(crate) draft: Option<Draft>,
}

/// The step under way: the draft folding it and the events it was given.
pub(crate) struct Draft {
    pub(crate) fold: StepDraft,
    pub(crate) events: Vec<StreamEvent>,
}

enum Message {
    Input(UserInput),
    Turn(Box<TurnRecord>),
}

struct Thread {
    origin: Origin,
    messages: Vec<Message>,
}

#[derive(Default)]
struct Inner {
    threads: BTreeMap<ThreadId, Thread>,
    log: Vec<String>,
}

/// The test thread store. Every operation is logged (`log`), and a test can
/// make a chosen call fail or run an action after it (`fail`, `after`).
#[derive(Default)]
pub struct TestStore {
    inner: Mutex<Inner>,
    pub(crate) faults: Faults,
}

impl TestStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// The operations done so far, one line each: `create {thread}`,
    /// `input {id}`, `open {turn}`, `event {turn}`, `results {turn}@{index}`,
    /// `end {turn} {status:?}`, `read {thread}`; a refused one ends with
    /// ` refused`.
    pub fn log(&self) -> Vec<String> {
        self.lock().log.clone()
    }

    /// The thread as `read` gives it, without logging.
    pub fn view(&self, thread: &ThreadId) -> Option<ThreadView> {
        self.lock()
            .threads
            .get(thread)
            .map(|thread| view(thread).expect("a thread of the test store renders"))
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `write` on the store, logged as `line`, unless the test injected
    /// a fault on this call of `op`; then runs what the test asked to run
    /// after it.
    fn write<T>(
        &self,
        op: Op,
        line: String,
        write: impl FnOnce(&mut Inner) -> Result<T, StoreError>,
    ) -> Result<T, ThreadError> {
        let call = self.faults.call(op);
        let result = if self.faults.fails(op, call) {
            Err(StoreError::Injected(op))
        } else {
            write(&mut self.lock())
        };
        let refused = if result.is_err() { " refused" } else { "" };
        self.lock().log.push(format!("{line}{refused}"));
        if result.is_ok() {
            self.faults.run_after(op, call);
        }
        result.map_err(|error| Box::new(error) as ThreadError)
    }
}

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

fn refused(error: impl std::fmt::Display) -> StoreError {
    StoreError::Refused(error.to_string())
}

/// The thread as a store reads it: its conversation, each turn with the
/// steps and results it holds, and its messages.
fn view(thread: &Thread) -> Result<ThreadView, StoreError> {
    let mut conversation = Conversation::new();
    let mut messages = Vec::new();
    for message in &thread.messages {
        match message {
            Message::Input(input) => {
                conversation.push_input(input.clone()).map_err(refused)?;
                messages.push(ThreadMessage::new(
                    input.id().as_str(),
                    input.author().cloned(),
                    MessageStatus::Completed,
                ));
            }
            Message::Turn(record) => {
                if let Some(turn) = &record.turn {
                    conversation.push_turn(turn.clone()).map_err(refused)?;
                }
                messages.push(ThreadMessage::new(
                    record.id.as_str(),
                    Some(record.author.clone()),
                    record.status,
                ));
            }
        }
    }
    Ok(ThreadView::new(conversation, messages))
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
