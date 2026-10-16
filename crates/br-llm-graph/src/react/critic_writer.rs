//! A message of the critic, recorded by the generator-critic itself: a new
//! turn of the critic holding one text step, written to the thread first
//! when durable, then applied to `conversation`.

use br_llm_messages::{AssistantBlock, Step, Text, Turn};

use crate::error::GraphError;
use crate::graph::Context;
use crate::react::agent_call::Position;
use crate::react::duo::{Duo, TurnKind};
use crate::react::thread_store::TurnEnd;
use crate::react::turn_writer::TurnWriter;
use crate::state::{State, Value};
use crate::update::Update;

/// Records a message of the critic of `kind`, holding `text`, as the next
/// turn of the call: for a durable generator-critic, opened in the thread,
/// streamed as a client would stream it and completed; then the updates that
/// add it to `conversation` and to the call's turns, and move the call to
/// its rank.
pub(crate) async fn record(
    duo: &Duo,
    state: &State,
    ctx: &Context,
    kind: TurnKind,
    text: Text,
) -> Result<Vec<Update>, GraphError> {
    let conversation = state.conversation(&duo.conversation)?;
    let (rank, id) = duo.call.next_turn(state, ctx, conversation)?;
    let stop_reason = kind.stop_reason();
    if let Some(binding) = &duo.call.thread {
        let writer = TurnWriter {
            store: binding.store.as_ref(),
            thread: binding.thread(state)?,
            turn: id.clone(),
        };
        writer.open(&duo.critic).await?;
        writer.push_text_step(&text, stop_reason.clone()).await?;
        writer.end(TurnEnd::Completed).await?;
    }
    let step = Step::new(vec![AssistantBlock::Text { text }], stop_reason, None, None)?;
    Ok(vec![
        Update::PushTurn {
            key: duo.conversation.clone(),
            turn: Turn::new(id.clone(), Some(duo.critic.clone()), step),
        },
        Update::Append {
            key: duo.turns.clone(),
            value: Value::str(id.as_str()),
        },
        duo.call.move_to(Position::Writing(rank)),
    ])
}
