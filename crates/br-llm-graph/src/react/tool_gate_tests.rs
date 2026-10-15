//! The tool concurrency gate: one entry per waiting call whatever the number
//! of polls, one waiter let through per freed place in arrival order, and a
//! cancelled waiter that holds no place.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Wake, Waker};

use super::Gate;

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
