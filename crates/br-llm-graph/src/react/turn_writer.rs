//! A durable agent's turn in its thread: the message its nodes write, under
//! the turn's id, before they apply what they wrote to `conversation`.

use br_llm_messages::{Author, BlockKind, Step, StopReason, StreamEvent, Text, ToolResult, TurnId};

use crate::error::GraphError;
use crate::react::thread_store::{ThreadStore, TurnEnd, thread_failed};
use crate::value::ThreadId;

/// The turn `turn` of the thread `thread`, in `store`.
pub(crate) struct TurnWriter<'a> {
    pub(crate) store: &'a dyn ThreadStore,
    pub(crate) thread: ThreadId,
    pub(crate) turn: TurnId,
}

impl TurnWriter<'_> {
    /// Opens the turn for `author`.
    pub(crate) async fn open(&self, author: &Author) -> Result<(), GraphError> {
        self.store
            .open_turn(&self.thread, &self.turn, author)
            .await
            .map_err(thread_failed)
    }

    /// Pushes a stream event of the step under way.
    pub(crate) async fn push_event(&self, event: &StreamEvent) -> Result<(), GraphError> {
        self.store
            .push_event(&self.thread, &self.turn, event)
            .await
            .map_err(thread_failed)
    }

    /// Pushes `events`, the events of a step held until it was checked, in
    /// order.
    pub(crate) async fn push_events(&self, events: &[StreamEvent]) -> Result<(), GraphError> {
        for event in events {
            self.push_event(event).await?;
        }
        Ok(())
    }

    /// Gives the store `step`, the step the client returned, once its stream
    /// has ended.
    pub(crate) async fn finish_step(&self, step: &Step) -> Result<(), GraphError> {
        self.store
            .finish_step(&self.thread, &self.turn, step)
            .await
            .map_err(thread_failed)
    }

    /// Pushes `results`, one tool results item, at `index` among the turn's
    /// items.
    pub(crate) async fn push_results(
        &self,
        index: usize,
        results: &[ToolResult],
    ) -> Result<(), GraphError> {
        self.store
            .push_results(&self.thread, &self.turn, index, results)
            .await
            .map_err(thread_failed)
    }

    /// Streams a step the library writes itself, one text block with
    /// `stop_reason`, as a client would stream it.
    pub(crate) async fn push_text_step(
        &self,
        text: &Text,
        stop_reason: StopReason,
    ) -> Result<(), GraphError> {
        self.push_events(&text_step_events(text, stop_reason)).await
    }

    /// Completes, fails or cancels the turn.
    pub(crate) async fn end(&self, end: TurnEnd) -> Result<(), GraphError> {
        self.store
            .end_turn(&self.thread, &self.turn, end)
            .await
            .map_err(thread_failed)
    }
}

/// The stream events of a step holding the one text block `text`, stopped
/// with `stop_reason`, without usage: folded, they give that step without a
/// model.
pub(crate) fn text_step_events(text: &Text, stop_reason: StopReason) -> Vec<StreamEvent> {
    vec![
        StreamEvent::BlockStart {
            index: 0,
            kind: BlockKind::Text,
        },
        StreamEvent::TextDelta {
            index: 0,
            text: text.as_str().to_owned(),
        },
        StreamEvent::BlockEnd { index: 0 },
        StreamEvent::Finish {
            stop_reason,
            usage: None,
        },
    ]
}
