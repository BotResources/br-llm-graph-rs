//! The tool concurrency limit: a gate that lets at most a number of tool
//! calls run at once, shared by the tool nodes of one ReAct loop.

use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

use crate::state::State;

/// Lets at most `width` holders through at once; a holder leaves when its
/// pass is dropped. Runtime-neutral: waiting tasks are woken through their
/// wakers.
///
/// Each waiting entry owns one place in an arrival queue, keeping one waker
/// whatever the number of its polls, and leaves the queue when dropped. A
/// freed place is handed to the first waiter in arrival order, which alone
/// is woken; a waiter handed a place it no longer wants passes it on. A
/// waiter queues and a place is freed under one lock: no wakeup is lost.
///
/// A waker is woken and dropped out of the lock: the last reference to a
/// task may own its future, whose drop can give back a place of this very
/// gate and so take its lock. A waker is cloned under the lock, as tokio's
/// semaphore does too: a clone only takes a reference.
pub(crate) struct Gate {
    inner: Mutex<GateState>,
}

struct GateState {
    /// Places neither held nor handed to a waiter.
    free: usize,
    /// The id the next waiter queues under.
    next: u64,
    /// The waiters, in arrival order, each with the waker of its last poll.
    queue: VecDeque<(u64, Waker)>,
    /// The waiters handed a place, not yet polled since.
    handed: HashSet<u64>,
}

impl GateState {
    /// Gives a place back: to the first waiter in arrival order, whose waker
    /// is returned to be woken out of the lock, or to the free places.
    fn release(&mut self) -> Option<Waker> {
        match self.queue.pop_front() {
            Some((id, waker)) => {
                self.handed.insert(id);
                Some(waker)
            }
            None => {
                self.free += 1;
                None
            }
        }
    }
}

impl Gate {
    pub(crate) fn new(width: usize) -> Self {
        Self {
            inner: Mutex::new(GateState {
                free: width,
                next: 0,
                queue: VecDeque::new(),
                handed: HashSet::new(),
            }),
        }
    }

    /// Waits for a place and holds it until the pass is dropped.
    pub(crate) fn enter(&self) -> Enter<'_> {
        Enter {
            gate: self,
            queued: None,
        }
    }

    fn release(&self) {
        let waker = lock(&self.inner).release();
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

/// The wait for a place in a gate: `queued` is its id once it waits.
pub(crate) struct Enter<'a> {
    gate: &'a Gate,
    queued: Option<u64>,
}

impl<'a> Future for Enter<'a> {
    type Output = Pass<'a>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Pass<'a>> {
        let gate = self.gate;
        let mut state = lock(&gate.inner);
        match self.queued {
            None if state.free > 0 => {
                state.free -= 1;
                Poll::Ready(Pass { gate })
            }
            None => {
                let id = state.next;
                state.next += 1;
                state.queue.push_back((id, cx.waker().clone()));
                drop(state);
                self.queued = Some(id);
                Poll::Pending
            }
            Some(id) if state.handed.remove(&id) => {
                drop(state);
                self.queued = None;
                Poll::Ready(Pass { gate })
            }
            Some(id) => {
                let mut replaced = None;
                if let Some((_, waker)) = state.queue.iter_mut().find(|(queued, _)| *queued == id)
                    && !waker.will_wake(cx.waker())
                {
                    replaced = Some(std::mem::replace(waker, cx.waker().clone()));
                }
                drop(state);
                drop(replaced);
                Poll::Pending
            }
        }
    }
}

impl Drop for Enter<'_> {
    /// A waiter dropped while it waits leaves the queue; one dropped after it
    /// was handed a place passes the place on.
    fn drop(&mut self) {
        let Some(id) = self.queued else {
            return;
        };
        let (removed, handed) = {
            let mut state = lock(&self.gate.inner);
            if state.handed.remove(&id) {
                (None, state.release())
            } else {
                let at = state.queue.iter().position(|(queued, _)| *queued == id);
                (at.and_then(|at| state.queue.remove(at)), None)
            }
        };
        drop(removed);
        if let Some(waker) = handed {
            waker.wake();
        }
    }
}

/// A place taken in a gate, given back when dropped.
pub(crate) struct Pass<'a> {
    gate: &'a Gate,
}

impl Drop for Pass<'_> {
    fn drop(&mut self) {
        self.gate.release();
    }
}

/// The gates of the tool nodes of one ReAct loop: one gate per superstep, so
/// the tool nodes running together share one limit while concurrent runs of
/// the same graph never share theirs. A superstep is told by the state its
/// nodes read, the same value for all of them while it runs.
#[derive(Default)]
pub(crate) struct SharedGates {
    gates: Mutex<HashMap<usize, (Arc<Gate>, usize)>>,
}

impl SharedGates {
    /// The gate of the superstep reading `state`, made with `width` places
    /// when no tool node of that superstep holds it yet.
    pub(crate) fn lease(self: &Arc<Self>, state: &State, width: usize) -> Lease {
        let superstep = std::ptr::from_ref(state) as usize;
        let mut gates = lock(&self.gates);
        let (gate, holders) = gates
            .entry(superstep)
            .or_insert_with(|| (Arc::new(Gate::new(width)), 0));
        *holders += 1;
        Lease {
            gate: gate.clone(),
            shared: self.clone(),
            superstep,
        }
    }
}

/// The superstep's shared gate, held by a tool node while it runs its calls
/// and forgotten once its last holder is done.
pub(crate) struct Lease {
    gate: Arc<Gate>,
    shared: Arc<SharedGates>,
    superstep: usize,
}

impl Lease {
    pub(crate) fn gate(&self) -> &Gate {
        &self.gate
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let mut gates = lock(&self.shared.gates);
        if let Some((_, holders)) = gates.get_mut(&self.superstep) {
            *holders -= 1;
            if *holders == 0 {
                gates.remove(&self.superstep);
            }
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
#[path = "tool_gate_tests.rs"]
mod tests;
