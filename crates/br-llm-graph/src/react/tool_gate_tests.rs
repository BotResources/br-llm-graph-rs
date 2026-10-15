//! The tool concurrency gate: one entry per waiting call whatever the number
//! of polls, at the gate and through a tool node; one waiter let through per
//! freed place in arrival order; a cancelled waiter that holds no place; and
//! a waker replaced or removed that is dropped out of the gate's lock.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll, Wake, Waker};

use br_llm_clients_core::ToolSpec;
use br_llm_messages::{Text, ToolResultBlock};
use serde_json::{Value as Json, json};

use super::Gate;
use crate::react::test_support::{EchoTool, spec};
use crate::react::tool::{Tool, ToolFuture, ToolOutput};
use crate::state::{Config, State};

/// A waker that counts its wakes; its strong count tells the live clones.
#[derive(Default)]
struct Counting {
    wakes: AtomicUsize,
}

impl Wake for Counting {
    fn wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
    }
}

fn counting() -> (Arc<Counting>, Waker) {
    let counter = Arc::new(Counting::default());
    let waker = Waker::from(counter.clone());
    (counter, waker)
}

/// A waker that notes, when its last clone goes, whether the lock of its
/// gate was free: the drop of a task's last waker may run code that gives a
/// place back to that gate.
struct DropProbe {
    gate: Arc<Gate>,
    lock_free_at_drop: Arc<OnceLock<bool>>,
}

impl Wake for DropProbe {
    fn wake(self: Arc<Self>) {
        unreachable!("the probed waiter is not let through");
    }
}

impl Drop for DropProbe {
    fn drop(&mut self) {
        let free = self.gate.inner.try_lock().is_ok();
        let _ = self.lock_free_at_drop.set(free);
    }
}

fn drop_probe(gate: &Arc<Gate>) -> (Arc<OnceLock<bool>>, Waker) {
    let lock_free_at_drop = Arc::new(OnceLock::new());
    let waker = Waker::from(Arc::new(DropProbe {
        gate: gate.clone(),
        lock_free_at_drop: lock_free_at_drop.clone(),
    }));
    (lock_free_at_drop, waker)
}

fn poll<F: Future + Unpin>(future: &mut F, waker: &Waker) -> Poll<F::Output> {
    Pin::new(future).poll(&mut Context::from_waker(waker))
}

#[test]
fn given_a_waiting_call_polled_many_times_when_it_waits_then_the_gate_keeps_one_waker_for_it() {
    let gate = Gate::new(1);
    let (_, holder_waker) = counting();
    let mut holder = Box::pin(gate.enter());
    let Poll::Ready(pass) = poll(&mut holder, &holder_waker) else {
        panic!("a free place is taken at once");
    };
    let (counter, waker) = counting();
    let mut waiting = Box::pin(gate.enter());

    for _ in 0..1000 {
        assert!(poll(&mut waiting, &waker).is_pending());
    }

    assert_eq!(
        Arc::strong_count(&counter),
        3,
        "the counter, the test's waker and the one clone the gate keeps for the waiting call"
    );
    drop(pass);
    assert_eq!(counter.wakes.load(Ordering::SeqCst), 1);
    assert!(poll(&mut waiting, &waker).is_ready());
}

#[test]
fn given_several_waiting_calls_when_a_place_frees_then_only_the_first_arrived_is_let_through() {
    let gate = Gate::new(1);
    let (_, holder_waker) = counting();
    let mut holder = Box::pin(gate.enter());
    let Poll::Ready(pass) = poll(&mut holder, &holder_waker) else {
        panic!("a free place is taken at once");
    };
    let waiters: Vec<(Arc<Counting>, Waker)> = (0..3).map(|_| counting()).collect();
    let mut entries: Vec<_> = (0..3).map(|_| Box::pin(gate.enter())).collect();
    for (entry, (_, waker)) in entries.iter_mut().zip(&waiters) {
        assert!(poll(entry, waker).is_pending());
    }

    drop(pass);

    let wakes: Vec<usize> = waiters
        .iter()
        .map(|(counter, _)| counter.wakes.load(Ordering::SeqCst))
        .collect();
    assert_eq!(wakes, [1, 0, 0], "one waiter woken per freed place");
    // A later arrival polled first does not jump the queue.
    assert!(poll(&mut entries[2], &waiters[2].1).is_pending());
    assert!(poll(&mut entries[1], &waiters[1].1).is_pending());
    assert!(poll(&mut entries[0], &waiters[0].1).is_ready());
}

#[test]
fn given_a_waiting_call_dropped_when_a_place_frees_then_the_next_waiter_gets_it() {
    let gate = Gate::new(1);
    let (_, holder_waker) = counting();
    let mut holder = Box::pin(gate.enter());
    let Poll::Ready(pass) = poll(&mut holder, &holder_waker) else {
        panic!("a free place is taken at once");
    };
    let (first, first_waker) = counting();
    let (second, second_waker) = counting();
    let mut cancelled = Box::pin(gate.enter());
    let mut next = Box::pin(gate.enter());
    assert!(poll(&mut cancelled, &first_waker).is_pending());
    assert!(poll(&mut next, &second_waker).is_pending());

    drop(cancelled);
    assert_eq!(
        Arc::strong_count(&first),
        2,
        "the counter and the test's waker: a cancelled waiter leaves the gate"
    );
    drop(pass);

    assert_eq!(first.wakes.load(Ordering::SeqCst), 0);
    assert_eq!(second.wakes.load(Ordering::SeqCst), 1);
    assert!(poll(&mut next, &second_waker).is_ready());
}

#[test]
fn given_a_waiter_let_through_then_dropped_before_its_poll_when_it_goes_then_its_place_passes_on() {
    let gate = Gate::new(1);
    let (_, holder_waker) = counting();
    let mut holder = Box::pin(gate.enter());
    let Poll::Ready(pass) = poll(&mut holder, &holder_waker) else {
        panic!("a free place is taken at once");
    };
    let (_, first_waker) = counting();
    let (second, second_waker) = counting();
    let mut let_through = Box::pin(gate.enter());
    let mut next = Box::pin(gate.enter());
    assert!(poll(&mut let_through, &first_waker).is_pending());
    assert!(poll(&mut next, &second_waker).is_pending());

    drop(pass);
    drop(let_through);

    assert_eq!(second.wakes.load(Ordering::SeqCst), 1);
    assert!(poll(&mut next, &second_waker).is_ready());
}

#[test]
fn given_a_waiting_call_polled_by_another_task_when_its_waker_is_replaced_then_the_old_one_goes_out_of_the_lock()
 {
    let gate = Arc::new(Gate::new(1));
    let (_, holder_waker) = counting();
    let mut holder = Box::pin(gate.enter());
    let Poll::Ready(pass) = poll(&mut holder, &holder_waker) else {
        panic!("a free place is taken at once");
    };
    let (lock_free_at_drop, first) = drop_probe(&gate);
    let mut waiting = Box::pin(gate.enter());
    assert!(poll(&mut waiting, &first).is_pending());
    drop(first);
    let (_, second) = counting();

    assert!(poll(&mut waiting, &second).is_pending());

    assert_eq!(
        lock_free_at_drop.get(),
        Some(&true),
        "the replaced waker, the last clone of the first task's, is dropped out of the lock"
    );
    drop(pass);
    assert!(poll(&mut waiting, &second).is_ready());
}

#[test]
fn given_a_waiting_call_dropped_when_it_leaves_the_queue_then_its_waker_goes_out_of_the_lock() {
    let gate = Arc::new(Gate::new(1));
    let (_, holder_waker) = counting();
    let mut holder = Box::pin(gate.enter());
    let Poll::Ready(_pass) = poll(&mut holder, &holder_waker) else {
        panic!("a free place is taken at once");
    };
    let (lock_free_at_drop, waker) = drop_probe(&gate);
    let mut waiting = Box::pin(gate.enter());
    assert!(poll(&mut waiting, &waker).is_pending());
    drop(waker);

    drop(waiting);

    assert_eq!(
        lock_free_at_drop.get(),
        Some(&true),
        "the waker of a waiter leaving the queue, its last clone, is dropped out of the lock"
    );
}

/// A future pending `n` times, waking its task each time, as a tool
/// streaming I/O does.
struct Busy(usize);

impl Future for Busy {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 == 0 {
            return Poll::Ready(());
        }
        self.0 -= 1;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

/// A tool busy for `{"yields": n}` polls of its task.
struct BusyTool(ToolSpec);

impl Tool for BusyTool {
    fn spec(&self) -> &ToolSpec {
        &self.0
    }

    fn call<'a>(
        &'a self,
        arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async move {
            Busy(usize::try_from(arguments["yields"].as_u64().unwrap_or(0)).unwrap_or(0)).await;
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("busy done").unwrap(),
            )]))
        })
    }
}

#[test]
fn given_calls_waiting_for_a_busy_slot_when_their_task_is_polled_again_and_again_then_the_gate_keeps_one_waker_per_waiting_call()
 {
    // R3b's probe: one call busy for 2000 polls holds the only slot while
    // twenty calls wait for it.
    use crate::react::react_hook_support::{answered, call, pending, run_tools, tool_graph};
    let mut calls = vec![call("c0", "busy", json!({ "yields": 2000 }))];
    calls.extend((1..=20).map(|index| call(&format!("c{index}"), "echo", json!({}))));
    let busy: Arc<dyn Tool> = Arc::new(BusyTool(spec("busy")));
    let graph = tool_graph(vec![busy, Arc::new(EchoTool)], Some(1), false);
    let (counter, waker) = counting();
    let mut cx = Context::from_waker(&waker);
    let mut run = Box::pin(run_tools(&graph, pending(calls)));

    let mut peak = 0;
    let outcome = loop {
        if let Poll::Ready(outcome) = run.as_mut().poll(&mut cx) {
            break outcome;
        }
        peak = peak.max(Arc::strong_count(&counter));
    };

    assert_eq!(answered(outcome).len(), 21);
    assert!(
        peak <= 25,
        "live waker clones peaked at {peak} for twenty waiting calls: the counter, the task's \
         waker, one per waiting call and a margin of three"
    );
}
