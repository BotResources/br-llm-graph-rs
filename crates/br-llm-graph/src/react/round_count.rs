//! The rounds a turn of an agent counts against its round limit: its own,
//! after those of the turns it continues. A turn of the same call that
//! follows one the thread holds failed or cancelled continues that turn's
//! count, so a failure (a reply the client refused after it streamed, a
//! crash) never resets the limit.

use br_llm_messages::{Conversation, Entry, ToolResult, Turn, TurnItem};

use crate::error::GraphError;
use crate::react::agent_call::AgentCall;
use crate::state::{State, Value};
use crate::update::Update;

impl AgentCall {
    /// The ids of the call's turns the thread holds failed or cancelled, as
    /// the private key lists them: none in memory, where no turn fails and
    /// the key is not declared.
    pub(crate) fn failed_turns<'a>(&self, state: &'a State) -> Result<Vec<&'a str>, GraphError> {
        if self.thread.is_none() {
            return Ok(Vec::new());
        }
        Ok(state
            .list(&self.failed)?
            .iter()
            .filter_map(|value| match value {
                Value::Str(id) => Some(id.as_str()),
                Value::Int(_)
                | Value::Float(_)
                | Value::Bool(_)
                | Value::List(_)
                | Value::Conversation(_) => None,
            })
            .collect())
    }

    /// The update that lists `ids` as the call's turns the thread holds
    /// failed or cancelled.
    pub(crate) fn list_failed<'a>(&self, ids: impl IntoIterator<Item = &'a str>) -> Update {
        Update::Set {
            key: self.failed.clone(),
            value: Value::list(ids.into_iter().map(Value::str).collect()),
        }
    }
}

/// `turn` and the turns it continues in `conversation`, oldest first: each
/// turn of its author right before it whose id `failed` lists, the call's
/// turns the thread holds failed or cancelled. A turn of another author, a
/// user input or a turn that did not fail ends the chain. A failed turn the
/// store left without a step is not in the conversation: it answered no
/// round, and the chain goes on before it.
pub(crate) fn continued<'a>(
    conversation: &'a Conversation,
    turn: &'a Turn,
    failed: &[&str],
) -> Vec<&'a Turn> {
    let entries = conversation.entries();
    let end = entries
        .iter()
        .position(|entry| entry.id() == turn.id().as_str())
        .unwrap_or(entries.len());
    let mut chain: Vec<&Turn> = entries
        .get(..end)
        .unwrap_or_default()
        .iter()
        .rev()
        .map_while(|entry| match entry {
            Entry::Turn(before)
                if before.author() == turn.author() && failed.contains(&before.id().as_str()) =>
            {
                Some(before)
            }
            Entry::Turn(_) | Entry::UserInput(_) => None,
        })
        .collect();
    chain.reverse();
    chain.push(turn);
    chain
}

/// The rounds `turns` answered, in order: their tool results items.
pub(crate) fn rounds<'a>(turns: &[&'a Turn]) -> impl Iterator<Item = &'a [ToolResult]> {
    turns
        .iter()
        .copied()
        .flat_map(Turn::items)
        .filter_map(|item| match item {
            TurnItem::ToolResults(results) => Some(results.results()),
            TurnItem::Step(_) => None,
        })
}
