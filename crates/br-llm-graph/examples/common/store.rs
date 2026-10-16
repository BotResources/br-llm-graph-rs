//! A thread store kept in memory: the host's port, as a real store would
//! implement it over its database. Each write is repeated safely by its own
//! rule, the events of a step fold into its turn, a failed or cancelled turn
//! is repaired (see `turn.rs`).

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use br_llm_graph::{
    MessageStatus, Origin, ThreadError, ThreadFuture, ThreadId, ThreadMessage, ThreadStore,
    ThreadView, TurnEnd,
};
use br_llm_messages::{
    Author, Conversation, StepDraft, StreamEvent, ToolResult, Turn, TurnId, TurnItem, UserInput,
};

use super::turn::{fold, repair};

/// An agent's turn: what it holds so far (`None` before its first step) and
/// the events of the step under way.
pub struct TurnRecord {
    pub id: TurnId,
    pub author: Author,
    pub status: MessageStatus,
    pub turn: Option<Turn>,
    pub events: Vec<StreamEvent>,
    pub draft: Option<StepDraft>,
}

enum Message {
    Input(UserInput),
    Turn(Box<TurnRecord>),
}

struct Thread {
    origin: Origin,
    messages: Vec<Message>,
}

/// The threads of the examples, in memory.
#[derive(Default)]
pub struct MemoryStore {
    threads: Mutex<BTreeMap<ThreadId, Thread>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Prints the messages of `thread`: id, author and status.
    pub fn print(&self, thread: &ThreadId) {
        let Some(view) = self.lock().get(thread).map(view) else {
            println!("  thread {thread}: absent");
            return;
        };
        println!("  thread {thread}:");
        for message in view.messages {
            let author = message
                .author
                .map_or("user".to_owned(), |a| a.name().to_string());
            println!("    {author:<8} {:?} {}", message.status, message.id);
        }
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<ThreadId, Thread>> {
        self.threads.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `write` on `thread`, which must exist.
    fn on<T>(
        &self,
        thread: &ThreadId,
        write: impl FnOnce(&mut Thread) -> Result<T, String>,
    ) -> ThreadFuture<'static, T>
    where
        T: Send + 'static,
    {
        let result = match self.lock().get_mut(thread) {
            Some(held) => write(held),
            None => Err(format!("thread {thread} is absent")),
        };
        Box::pin(async move { result.map_err(ThreadError::from) })
    }
}

fn view(thread: &Thread) -> ThreadView {
    let mut conversation = Conversation::new();
    let mut messages = Vec::new();
    for message in &thread.messages {
        match message {
            Message::Input(input) => {
                drop(conversation.push_input(input.clone()));
                messages.push(ThreadMessage::new(
                    input.id().as_str(),
                    input.author().cloned(),
                    MessageStatus::Completed,
                ));
            }
            Message::Turn(record) => {
                if let Some(turn) = &record.turn {
                    drop(conversation.push_turn(turn.clone()));
                }
                messages.push(ThreadMessage::new(
                    record.id.as_str(),
                    Some(record.author.clone()),
                    record.status,
                ));
            }
        }
    }
    ThreadView::new(conversation, messages)
}

fn open_turn<'a>(thread: &'a mut Thread, id: &TurnId) -> Result<&'a mut TurnRecord, String> {
    thread
        .messages
        .iter_mut()
        .find_map(|message| match message {
            Message::Turn(record) if &record.id == id => Some(record.as_mut()),
            Message::Turn(_) | Message::Input(_) => None,
        })
        .ok_or_else(|| format!("turn {id} is not in the thread"))
}

fn holds(thread: &Thread, id: &str) -> bool {
    thread.messages.iter().any(|message| match message {
        Message::Input(input) => input.id().as_str() == id,
        Message::Turn(record) => record.id.as_str() == id,
    })
}

impl ThreadStore for MemoryStore {
    fn read<'a>(&'a self, thread: &'a ThreadId) -> ThreadFuture<'a, Option<ThreadView>> {
        let view = self.lock().get(thread).map(view);
        Box::pin(async move { Ok(view) })
    }

    fn create<'a>(&'a self, thread: &'a ThreadId, origin: &'a Origin) -> ThreadFuture<'a, ()> {
        let result = match self.lock().entry(thread.clone()) {
            std::collections::btree_map::Entry::Occupied(held) if &held.get().origin == origin => {
                Ok(())
            }
            std::collections::btree_map::Entry::Occupied(_) => {
                Err(format!("thread {thread} was created for another agent"))
            }
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(Thread {
                    origin: origin.clone(),
                    messages: Vec::new(),
                });
                Ok(())
            }
        };
        Box::pin(async move { result.map_err(ThreadError::from) })
    }

    fn store_input<'a>(
        &'a self,
        thread: &'a ThreadId,
        input: &'a UserInput,
    ) -> ThreadFuture<'a, ()> {
        let input = input.clone();
        self.on(thread, move |held| {
            let stored = held.messages.iter().find_map(|message| match message {
                Message::Input(stored) if stored.id() == input.id() => Some(stored == &input),
                Message::Input(_) | Message::Turn(_) => None,
            });
            match stored {
                Some(true) => Ok(()),
                Some(false) => Err(format!("input {} is held with another content", input.id())),
                None if holds(held, input.id().as_str()) => {
                    Err(format!("{} is the id of a turn", input.id()))
                }
                None => {
                    held.messages.push(Message::Input(input));
                    Ok(())
                }
            }
        })
    }

    fn open_turn<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        author: &'a Author,
    ) -> ThreadFuture<'a, ()> {
        let (turn, author) = (turn.clone(), author.clone());
        self.on(thread, move |held| {
            if let Ok(record) = open_turn(held, &turn) {
                return if record.author == author {
                    Ok(())
                } else {
                    Err(format!("turn {turn} is held for another author"))
                };
            }
            if holds(held, turn.as_str()) {
                return Err(format!("{turn} is the id of an input"));
            }
            {
                held.messages.push(Message::Turn(Box::new(TurnRecord {
                    id: turn,
                    author,
                    status: MessageStatus::Streaming,
                    turn: None,
                    events: Vec::new(),
                    draft: None,
                })));
                Ok(())
            }
        })
    }

    fn push_event<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        event: &'a StreamEvent,
    ) -> ThreadFuture<'a, ()> {
        let (turn, event) = (turn.clone(), event.clone());
        self.on(thread, move |held| {
            let record = open_turn(held, &turn)?;
            if record.status != MessageStatus::Streaming {
                return Err(format!("turn {turn} is not open"));
            }
            fold(record, &event)
        })
    }

    fn push_results<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        index: usize,
        results: &'a [ToolResult],
    ) -> ThreadFuture<'a, ()> {
        let (turn, results) = (turn.clone(), results.to_vec());
        self.on(thread, move |held| {
            let record = open_turn(held, &turn)?;
            let Some(held_turn) = &mut record.turn else {
                return Err(format!("turn {turn} has no step"));
            };
            match held_turn.items().get(index) {
                Some(TurnItem::ToolResults(_)) => Ok(()),
                Some(TurnItem::Step(_)) => Err(format!("item {index} of turn {turn} is a step")),
                None => held_turn.push_results(results).map_err(|e| e.to_string()),
            }
        })
    }

    fn end_turn<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        end: TurnEnd,
    ) -> ThreadFuture<'a, ()> {
        let turn = turn.clone();
        self.on(thread, move |held| {
            let record = open_turn(held, &turn)?;
            match record.status {
                MessageStatus::Streaming => {}
                status if status == end.status() => return Ok(()),
                status => return Err(format!("turn {turn} is already {status:?}")),
            }
            if matches!(end, TurnEnd::Failed | TurnEnd::Cancelled) {
                repair(record);
            }
            record.draft = None;
            record.status = end.status();
            Ok(())
        })
    }
}
