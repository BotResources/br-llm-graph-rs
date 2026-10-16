//! The requests an agent asks its ids under: its origin, its trigger and the
//! rank of each id within its call.

use br_llm_messages::{Conversation, Entry, InputId, UserInput};

use crate::graph::{Context, IdRequest};
use crate::origin::Origin;

/// The origin of the agent a node of its graph belongs to: the run id, and
/// the occurrence of the run the node runs in, which is the occurrence of
/// the node that calls the agent's graph (empty at top level). Every node of
/// one call asks its ids under it.
pub(crate) fn agent_origin(ctx: &Context) -> Origin {
    Origin {
        run: ctx.run_id().clone(),
        occurrence: ctx.occurrence().parent(),
    }
}

/// The trigger of a call: the last user input of `conversation`.
pub(crate) fn trigger(conversation: &Conversation) -> Option<&UserInput> {
    conversation
        .entries()
        .iter()
        .rev()
        .find_map(|entry| match entry {
            Entry::UserInput(input) => Some(input),
            Entry::Turn(_) => None,
        })
}

/// The request of the id of rank `rank` in the call that `ctx`'s agent
/// makes to answer `trigger`.
pub(crate) fn id_request(ctx: &Context, trigger: Option<&InputId>, rank: usize) -> IdRequest {
    IdRequest::new(agent_origin(ctx), trigger.cloned(), rank)
}
