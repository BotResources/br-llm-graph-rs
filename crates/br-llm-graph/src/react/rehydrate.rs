//! The entry of a durable agent into its thread: the thread found or
//! created, the opening stored, and the turns of this call found by their
//! ids, rank after rank, a turn left open failed so the store repairs it.

use br_llm_messages::{Conversation, Entry, TurnId, UserInput};

use crate::error::GraphError;
use crate::graph::Context;
use crate::react::agent_call::ThreadBinding;
use crate::react::thread_store::{MessageStatus, ThreadView, TurnEnd};
use crate::react::turn_ids::{agent_origin, id_request};
use crate::value::ThreadId;

/// What a durable agent finds on entry.
pub(crate) struct Entered {
    /// The thread it works in.
    pub(crate) thread: ThreadId,
    /// The thread as it stands, the opening stored: what the agent keeps in
    /// `conversation`.
    pub(crate) conversation: Conversation,
    /// The turns of this call the thread holds, rank after rank: each rank,
    /// id and status (a last turn found open has been failed and repaired).
    pub(crate) turns: Vec<(usize, TurnId, MessageStatus)>,
}

impl Entered {
    /// The last of the turns of this call the thread holds, with its rank
    /// and its status.
    pub(crate) fn last(&self) -> Option<(usize, MessageStatus)> {
        self.turns.last().map(|(rank, _, status)| (*rank, *status))
    }
}

/// The user inputs of a durable agent's `opening`, ending with its trigger:
/// an opening that holds none, or anything else, is refused
/// (`OpeningNotInputs`).
pub(crate) fn opening_inputs(opening: &Conversation) -> Result<Vec<&UserInput>, GraphError> {
    let inputs: Vec<&UserInput> = opening
        .entries()
        .iter()
        .map(|entry| match entry {
            Entry::UserInput(input) => Ok(input),
            Entry::Turn(_) => Err(GraphError::OpeningNotInputs),
        })
        .collect::<Result<_, _>>()?;
    if inputs.is_empty() {
        return Err(GraphError::OpeningNotInputs);
    }
    Ok(inputs)
}

/// Enters the thread named by `given` (empty: one the store creates under an
/// id from the id source) for the call answering the last input of
/// `inputs`.
///
/// A thread id that is not a UUIDv7 is refused (`ThreadId`), a thread the
/// store does not hold too (`ThreadNotFound`): the library never creates a
/// thread it was given. Each input the thread does not hold under its id is
/// stored. The thread is read once, and once more after the last turn of the
/// call, found open, is failed; the agent decides on statuses only.
pub(crate) async fn enter_thread(
    ctx: &Context,
    binding: &ThreadBinding,
    given: &str,
    inputs: &[&UserInput],
) -> Result<Entered, GraphError> {
    let trigger = inputs.last().map(|input| input.id());
    let store = binding.store.as_ref();
    let thread = if given.is_empty() {
        let thread = ctx.ids.thread_id(&id_request(ctx, trigger, 0));
        store.create(&thread, &agent_origin(ctx)).await?;
        thread
    } else {
        ThreadId::new(given)?
    };
    let mut view = read(binding, &thread).await?;
    for input in inputs {
        if view.message(input.id().as_str()).is_none() {
            store.store_input(&thread, input).await?;
            view.conversation.push_input((*input).clone())?;
        }
    }

    let mut turns = Vec::new();
    for rank in 1.. {
        let id = ctx.ids.turn_id(&id_request(ctx, trigger, rank));
        let Some(message) = view.message(id.as_str()) else {
            break;
        };
        let status = message.status;
        turns.push((rank, id, status));
    }
    if let Some((_, id, status)) = turns.last_mut()
        && *status == MessageStatus::Streaming
    {
        store.end_turn(&thread, id, TurnEnd::Failed).await?;
        view = read(binding, &thread).await?;
        *status = view
            .message(id.as_str())
            .map_or(MessageStatus::Failed, |message| message.status);
    }
    Ok(Entered {
        thread,
        conversation: view.conversation,
        turns,
    })
}

/// The thread `thread`, refused when the store does not hold it.
async fn read(binding: &ThreadBinding, thread: &ThreadId) -> Result<ThreadView, GraphError> {
    binding
        .store
        .read(thread)
        .await?
        .ok_or_else(|| GraphError::ThreadNotFound {
            thread: thread.clone(),
        })
}
