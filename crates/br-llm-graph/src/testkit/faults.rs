//! The faults a test injects into the test store: a chosen call of an
//! operation that fails, or an action run right after it succeeds (a cancel
//! sent between a write and what follows it, for example).

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::testkit::store::TestStore;

/// An operation of the thread store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Op {
    Read,
    Create,
    StoreInput,
    OpenTurn,
    PushEvent,
    PushResults,
    EndTurn,
}

type Action = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct Planned {
    /// The calls made so far, per operation.
    calls: HashMap<Op, usize>,
    /// The calls that fail, by operation and 1-based number.
    fail: HashSet<(Op, usize)>,
    /// The actions to run after a call succeeds.
    after: Vec<(Op, usize, Action)>,
}

#[derive(Default)]
pub(crate) struct Faults {
    planned: Mutex<Planned>,
}

impl Faults {
    fn lock(&self) -> MutexGuard<'_, Planned> {
        self.planned.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Counts a call of `op`: its 1-based number.
    pub(crate) fn call(&self, op: Op) -> usize {
        let mut planned = self.lock();
        let calls = planned.calls.entry(op).or_insert(0);
        *calls += 1;
        *calls
    }

    /// Whether call `call` of `op` fails.
    pub(crate) fn fails(&self, op: Op, call: usize) -> bool {
        self.lock().fail.contains(&(op, call))
    }

    /// Runs the actions planned after call `call` of `op`, out of the lock.
    pub(crate) fn run_after(&self, op: Op, call: usize) {
        let actions: Vec<Action> = {
            let mut planned = self.lock();
            let (due, kept) = std::mem::take(&mut planned.after)
                .into_iter()
                .partition(|(planned_op, number, _)| *planned_op == op && *number == call);
            planned.after = kept;
            due.into_iter().map(|(_, _, action)| action).collect()
        };
        for action in actions {
            action();
        }
    }

    fn nth_from_now(&self, op: Op, nth: usize) -> usize {
        self.lock().calls.get(&op).copied().unwrap_or(0) + nth
    }
}

impl TestStore {
    /// The `nth` call of `op` from now (1: the next one) fails with
    /// `StoreError::Injected`, and writes nothing.
    pub fn fail(&self, op: Op, nth: usize) {
        let call = self.faults.nth_from_now(op, nth);
        self.faults.lock().fail.insert((op, call));
    }

    /// Runs `action` right after the `nth` call of `op` from now succeeds.
    pub fn after(&self, op: Op, nth: usize, action: impl FnOnce() + Send + 'static) {
        let call = self.faults.nth_from_now(op, nth);
        self.faults.lock().after.push((op, call, Box::new(action)));
    }

    /// The calls of `op` made so far.
    pub fn calls(&self, op: Op) -> usize {
        self.faults.lock().calls.get(&op).copied().unwrap_or(0)
    }
}
