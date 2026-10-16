//! The common subjects, ready-made: fixed texts, texts read from a key, a
//! fixed client and schedules of clients.

use std::sync::Arc;

use br_llm_clients_core::Client;

use crate::react::given::{Budgeted, Given, ModelNode, Reads, Scheduled};
use crate::state::{ConfigKey, StateKey};

/// The items of a list of strings, joined with a blank line.
fn joined(items: Vec<&str>) -> String {
    items.join("\n\n")
}

impl<N: ModelNode> Given<N, String> {
    /// The fixed text `text`.
    pub fn text(text: impl Into<String>) -> Self {
        Self::value(text.into())
    }

    /// The text the state key `key` holds.
    pub fn from_state(key: &StateKey<String>) -> Self {
        let key = key.clone();
        Self::ready_made(Reads::of_state(&key), move |ctx| {
            Ok(ctx.get(&key)?.to_owned())
        })
    }

    /// The strings the state key `key` holds, joined with a blank line.
    pub fn from_state_list(key: &StateKey<Vec<String>>) -> Self {
        let key = key.clone();
        Self::ready_made(Reads::of_state(&key), move |ctx| Ok(joined(ctx.get(&key)?)))
    }

    /// The text the configuration key `key` holds.
    pub fn from_config(key: &ConfigKey<String>) -> Self {
        let key = key.clone();
        Self::ready_made(Reads::of_config(&key), move |ctx| {
            Ok(ctx.config(&key)?.to_owned())
        })
    }

    /// The strings the configuration key `key` holds, joined with a blank
    /// line.
    pub fn from_config_list(key: &ConfigKey<Vec<String>>) -> Self {
        let key = key.clone();
        Self::ready_made(Reads::of_config(&key), move |ctx| {
            Ok(joined(ctx.config(&key)?))
        })
    }
}

impl<N: ModelNode> Given<N, Arc<dyn Client>> {
    /// The fixed client `client`, called on every call.
    pub fn client(client: Arc<dyn Client>) -> Self {
        Self::value(client)
    }
}

impl<N: Scheduled> Given<N, Arc<dyn Client>> {
    /// A schedule of clients from the start: the first call calls `first`,
    /// the next ones each client of `then` in turn, and the last client
    /// repeats. The calls are counted by the node (`Scheduled`).
    pub fn schedule(first: Arc<dyn Client>, then: Vec<Arc<dyn Client>>) -> Self {
        Self::ready_made(Reads::new(), move |ctx| {
            Ok(from_start(&first, &then, ctx.call_index()).clone())
        })
    }
}

impl<N: Budgeted> Given<N, Arc<dyn Client>> {
    /// A schedule of clients ending with a tail: every call calls `primary`,
    /// but the last calls the node's budget allows, which call the clients of
    /// `tail`, in order, the last of them on the last call allowed. A call
    /// past the budget calls the last client of the tail.
    ///
    /// Only the two seats of the generator-critic have a budget
    /// (`Budgeted`): the generator's calls are its generations, revisions not
    /// counted (the budget plus one with `final_generation`, else the budget,
    /// one for a budget of 0); the critic's are the budget, verdict retries
    /// not counted. An LLM call and a ReAct agent have none, so they have no
    /// tail.
    pub fn ending_with(primary: Arc<dyn Client>, tail: Vec<Arc<dyn Client>>) -> Self {
        Self::ready_made(Reads::new(), move |ctx| {
            Ok(with_tail(&primary, &tail, ctx.call_index(), ctx.calls_allowed()).clone())
        })
    }
}

/// The client of the call at `index` of a schedule from the start.
pub(crate) fn from_start<'c, C>(first: &'c C, then: &'c [C], index: usize) -> &'c C {
    match index.checked_sub(1) {
        None => first,
        Some(rest) => then.get(rest).or_else(|| then.last()).unwrap_or(first),
    }
}

/// The client of the call at `index` of a schedule ending with `tail`, when
/// the budget allows `allowed` calls.
pub(crate) fn with_tail<'c, C>(
    primary: &'c C,
    tail: &'c [C],
    index: usize,
    allowed: usize,
) -> &'c C {
    if index >= allowed {
        return tail.last().unwrap_or(primary);
    }
    // The calls left after this one, this one included: 1 on the last call.
    let left = allowed - index;
    match tail.len().checked_sub(left) {
        Some(position) => tail.get(position).unwrap_or(primary),
        None => primary,
    }
}

#[cfg(test)]
#[path = "ready_tests.rs"]
mod tests;
