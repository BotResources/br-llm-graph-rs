//! The thread store, the port through which an agent reads and writes its
//! thread, and what a read gives back.

use std::future::Future;
use std::pin::Pin;

use br_llm_messages::{Author, Conversation, StreamEvent, ToolResult, TurnId, UserInput};

use crate::origin::Origin;
use crate::value::ThreadId;

/// The error of a thread store, of the host's type: it fails the agent's
/// node with `GraphError::Thread`, where the host can downcast it.
pub type ThreadError = Box<dyn std::error::Error + Send + Sync>;

/// One operation of a thread store under way.
pub type ThreadFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ThreadError>> + Send + 'a>>;

/// Where a message of a thread stands. A user input is `Completed` once
/// stored; an agent's turn is `Streaming` from the moment it is opened until
/// it is completed, failed or cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageStatus {
    Streaming,
    Completed,
    Failed,
    Cancelled,
}

/// How an open turn ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnEnd {
    Completed,
    Failed,
    Cancelled,
}

impl TurnEnd {
    /// The status of a turn ended this way.
    pub fn status(self) -> MessageStatus {
        match self {
            TurnEnd::Completed => MessageStatus::Completed,
            TurnEnd::Failed => MessageStatus::Failed,
            TurnEnd::Cancelled => MessageStatus::Cancelled,
        }
    }
}

/// One message of a thread: a user input or an agent's turn, under its id.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ThreadMessage {
    /// The id of the input or of the turn.
    pub id: String,
    pub author: Option<Author>,
    pub status: MessageStatus,
}

impl ThreadMessage {
    /// The message under `id`, of `author`, at `status`.
    pub fn new(id: impl Into<String>, author: Option<Author>, status: MessageStatus) -> Self {
        Self {
            id: id.into(),
            author,
            status,
        }
    }
}

/// A thread as a store reads it: its active branch as a conversation, not
/// lightened, and each of its messages with its id, author and status, in
/// the order of the thread.
///
/// A message may be missing from the conversation: a turn whose first step
/// is still streaming, or a turn left without any step once repaired. The
/// conversation holds the finished steps and the tool results of every
/// other turn, open ones included.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ThreadView {
    pub conversation: Conversation,
    pub messages: Vec<ThreadMessage>,
}

impl ThreadView {
    /// The thread read as `conversation`, holding `messages`.
    pub fn new(conversation: Conversation, messages: Vec<ThreadMessage>) -> Self {
        Self {
            conversation,
            messages,
        }
    }

    /// The message under `id`, if the thread holds one.
    pub fn message(&self, id: &str) -> Option<&ThreadMessage> {
        self.messages.iter().find(|message| message.id == id)
    }
}

/// The store an agent works in, the host's port: an agent built with one is
/// durable, it works in a thread and resumes from it.
///
/// What a store does:
/// - `read` gives the thread's active branch as a conversation, not
///   lightened, with each message's id, author and status, or `None` when
///   the thread is absent;
/// - `create` creates a thread under an id, given the origin of the asking
///   agent; whether it is a root or a child of another thread, and its
///   owner, members and resources, are the host's decision;
/// - `store_input` stores a user input under its id;
/// - `open_turn` opens a turn under its id for an AI author;
/// - `push_event` pushes a stream event of a step into an open turn: the
///   store folds the events of each step and adds the step at its finish;
/// - `push_results` pushes a tool results item into an open turn, whole,
///   under its index among the turn's items;
/// - `end_turn` completes, fails or cancels an open turn. After a turn is
///   failed or cancelled, the store repairs it: every tool call without a
///   result is answered with an error result saying the call was
///   interrupted, and an unfinished step is kept when br-llm-messages can
///   finish it, else dropped, so the thread always renders.
///
/// Each write is repeated safely by its own rule:
/// - creating a thread, storing a user input or opening a turn under an id
///   the store already holds is already done (`Ok`); with other content
///   (another origin, another input, another author) it is an error;
/// - a tool results item already held at its index is already done;
/// - ending a turn already in that state is already done; a turn in another
///   final state is an error.
///
/// A turn is written only by the run that opened it: an agent that finds
/// its turn open on entry fails it, never continues it, so its stream
/// events are never pushed twice. An error of the store fails the agent's
/// node with `GraphError::Thread`, carrying it.
pub trait ThreadStore: Send + Sync {
    /// The thread `thread`, or `None` when it is absent.
    fn read<'a>(&'a self, thread: &'a ThreadId) -> ThreadFuture<'a, Option<ThreadView>>;

    /// Creates the thread `thread` for the agent at `origin`.
    fn create<'a>(&'a self, thread: &'a ThreadId, origin: &'a Origin) -> ThreadFuture<'a, ()>;

    /// Stores `input` at the end of `thread`, under its id.
    fn store_input<'a>(
        &'a self,
        thread: &'a ThreadId,
        input: &'a UserInput,
    ) -> ThreadFuture<'a, ()>;

    /// Opens the turn `turn` of `author` at the end of `thread`: a message
    /// that streams until it ends.
    fn open_turn<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        author: &'a Author,
    ) -> ThreadFuture<'a, ()>;

    /// Pushes a stream event of the step under way into the open turn.
    fn push_event<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        event: &'a StreamEvent,
    ) -> ThreadFuture<'a, ()>;

    /// Pushes `results`, one tool results item, into the open turn at
    /// `index` among its items.
    fn push_results<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        index: usize,
        results: &'a [ToolResult],
    ) -> ThreadFuture<'a, ()>;

    /// Completes, fails or cancels the open turn; a failed or cancelled turn
    /// is repaired.
    fn end_turn<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        end: TurnEnd,
    ) -> ThreadFuture<'a, ()>;
}
