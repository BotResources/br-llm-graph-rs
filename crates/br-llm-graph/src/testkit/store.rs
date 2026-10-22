//! A thread store for tests that behaves as a real store: it keeps threads
//! in memory and reads them back; its operations are in `store_port.rs`,
//! the repair in `repair.rs`, the faults a test injects in `faults.rs`.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use br_llm_messages::{
    Author, Conversation, Step, StepDraft, StreamEvent, Turn, TurnId, UserInput,
};

use crate::origin::Origin;
use crate::react::{MessageStatus, ThreadError, ThreadMessage, ThreadView};
use crate::testkit::faults::{Faults, Op};
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
/// first step), the events of the step under way, and the steps the library
/// gave once their stream ended, with their model and usage.
pub(crate) struct TurnRecord {
    pub(crate) id: TurnId,
    pub(crate) author: Author,
    pub(crate) status: MessageStatus,
    pub(crate) turn: Option<Turn>,
    pub(crate) draft: Option<Draft>,
    pub(crate) finished: Vec<Step>,
}

/// The step under way: the draft folding it and the events it was given.
pub(crate) struct Draft {
    pub(crate) fold: StepDraft,
    pub(crate) events: Vec<StreamEvent>,
}

pub(crate) enum Message {
    Input(UserInput),
    Turn(Box<TurnRecord>),
}

pub(crate) struct Thread {
    pub(crate) origin: Origin,
    pub(crate) messages: Vec<Message>,
}

#[derive(Default)]
pub(crate) struct Inner {
    pub(crate) threads: BTreeMap<ThreadId, Thread>,
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
    /// `input {id}`, `open {turn}`, `event {turn}`, `finish {turn}`,
    /// `results {turn}@{index}`, `end {turn} {status:?}`, `read {thread}`; a
    /// refused one ends with ` refused`.
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

    /// The steps the library gave for the turn `turn` of `thread` once their
    /// stream ended (`finish_step`), in order, each with its model and its
    /// usage; none for a turn the store does not hold.
    pub fn finished(&self, thread: &ThreadId, turn: &TurnId) -> Vec<Step> {
        let inner = self.lock();
        let Some(held) = inner.threads.get(thread) else {
            return Vec::new();
        };
        held.messages
            .iter()
            .find_map(|message| match message {
                Message::Turn(record) if &record.id == turn => Some(record.finished.clone()),
                Message::Turn(_) | Message::Input(_) => None,
            })
            .unwrap_or_default()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `write` on the store, logged as `line`, unless the test injected
    /// a fault on this call of `op`; then runs what the test asked to run
    /// after it.
    pub(crate) fn write<T>(
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

/// The thread as a store reads it: its conversation, each turn with the
/// steps and results it holds, and its messages.
pub(crate) fn view(thread: &Thread) -> Result<ThreadView, StoreError> {
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

pub(crate) fn refused(error: impl std::fmt::Display) -> StoreError {
    StoreError::Refused(error.to_string())
}
