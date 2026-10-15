//! The tool concurrency limit: a gate that lets at most a number of tool
//! calls run at once, shared by the tool nodes of one ReAct loop.

use std::collections::HashMap;
use std::future::poll_fn;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Poll, Waker};

use crate::state::State;

/// Lets at most `width` holders through at once; a holder leaves when its
/// pass is dropped. Runtime-neutral: waiting tasks are woken through their
/// wakers.
pub(crate) struct Gate {
    inner: Mutex<GateState>,
}

struct GateState {
    free: usize,
    waiting: Vec<Waker>,
}

impl Gate {
    pub(crate) fn new(width: usize) -> Self {
        Self {
            inner: Mutex::new(GateState {
                free: width,
                waiting: Vec::new(),
            }),
        }
    }

    /// Waits for a free place and takes it until the pass is dropped.
    pub(crate) async fn enter(&self) -> Pass<'_> {
        poll_fn(|cx| {
            let mut state = lock(&self.inner);
            if state.free > 0 {
                state.free -= 1;
                Poll::Ready(())
            } else {
                state.waiting.push(cx.waker().clone());
                Poll::Pending
            }
        })
        .await;
        Pass { gate: self }
    }
}

/// A place taken in a gate, given back when dropped.
pub(crate) struct Pass<'a> {
    gate: &'a Gate,
}

impl Drop for Pass<'_> {
    fn drop(&mut self) {
        let waiting = {
            let mut state = lock(&self.gate.inner);
            state.free += 1;
            std::mem::take(&mut state.waiting)
        };
        for waker in waiting {
            waker.wake();
        }
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
