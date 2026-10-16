//! The private keys of a graph: state keys its runs work on that never leave
//! a run. An agent's `conversation` is one: neither an input nor an output,
//! never part of a checkpoint.
//!
//! A run takes and gives back states of the graph's schema, which does not
//! declare the private keys. It works on a state of the run schema, where
//! each private key starts at its start value, and takes them out of every
//! state it gives back: the final state, the state of each checkpoint and
//! the state of each observer `checkpoint` event.
//!
//! Since its private keys are lost between two runs, a run of such a graph
//! that is resumed enters again through the graph's entry: every checkpoint
//! it gives names the entry as its cursor (an empty cursor at the end stays
//! empty), and holds no pending entry, since a resumed run never reuses
//! what a node of the interrupted superstep recorded. Its runs record
//! nothing.

use crate::graph::Graph;
use crate::run::Cursor;
use crate::state::State;

impl Graph {
    /// Whether the graph has private keys.
    pub(crate) fn has_private(&self) -> bool {
        !self.private().is_empty()
    }

    /// The state a run works on: `state` with each private key at its start
    /// value.
    pub(crate) fn enter_private(&self, state: State) -> State {
        if !self.has_private() || state.schema() == self.run_schema() {
            return state;
        }
        state.extended(self.run_schema())
    }

    /// What a run gives back of `state`: the state without the private keys.
    pub(crate) fn leave_private(&self, state: State) -> State {
        if !self.has_private() || state.schema() == self.schema() {
            return state;
        }
        state.restricted(self.schema())
    }

    /// The cursor a checkpoint of a run of this graph names, the run standing
    /// at `cursor`: the entry for a graph with private keys, unless the run
    /// has ended.
    pub(crate) fn checkpoint_cursor(&self, cursor: Cursor) -> Cursor {
        if !self.has_private() || cursor.is_empty() {
            return cursor;
        }
        Cursor::new(vec![self.entry().clone()], Vec::new())
    }
}

#[cfg(test)]
#[path = "private_tests.rs"]
mod tests;
