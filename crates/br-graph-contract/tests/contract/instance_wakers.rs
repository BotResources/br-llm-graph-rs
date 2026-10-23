#![allow(clippy::panic)]

use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::pin::pin;
use std::sync::Arc;
use std::task::{Wake, Waker};

use br_graph_contract::{Event, Instance};

use crate::common::{Tally, TallyUpdate, id};
use crate::park::{counting_waker, poll_with};

struct PanicsOnWake;

impl Wake for PanicsOnWake {
    fn wake(self: Arc<Self>) {
        panic_any("the waker panicked");
    }

    fn wake_by_ref(self: &Arc<Self>) {
        panic_any("the waker panicked");
    }
}

fn panicking_waker() -> Waker {
    Waker::from(Arc::new(PanicsOnWake))
}

fn raised(payload: Box<dyn std::any::Any + Send>) -> bool {
    payload.downcast_ref::<&str>() == Some(&"the waker panicked")
}

#[test]
fn given_a_panicking_reader_waker_when_the_sender_finishes_then_the_event_waiter_is_still_woken() {
    let (instance, mut reader, sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, mut events) = instance.into_parts();
    let (wakes, counting) = counting_waker();
    let mut next = pin!(reader.next());
    let mut recv = pin!(events.recv());
    assert!(poll_with(next.as_mut(), &panicking_waker()).is_pending());
    assert!(poll_with(recv.as_mut(), &counting).is_pending());
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        sender.finish(Event::Result(TallyUpdate::Nothing));
    }));
    assert!(raised(outcome.unwrap_err()));
    assert_eq!(wakes.count(), 1);
}

#[test]
fn given_a_panicking_reader_waker_when_the_input_end_is_dropped_then_the_dropped_waiter_is_still_woken()
 {
    let (instance, mut reader, sender) = Instance::<Tally>::channel(id(1));
    let (input_end, _events) = instance.into_parts();
    let (wakes, counting) = counting_waker();
    let mut next = pin!(reader.next());
    let mut dropped = pin!(sender.dropped());
    assert!(poll_with(next.as_mut(), &panicking_waker()).is_pending());
    assert!(poll_with(dropped.as_mut(), &counting).is_pending());
    let outcome = catch_unwind(AssertUnwindSafe(|| drop(input_end)));
    assert!(raised(outcome.unwrap_err()));
    assert_eq!(wakes.count(), 1);
    assert!(poll_with(dropped.as_mut(), &counting).is_ready());
}

#[test]
fn given_a_panicking_reader_waker_when_the_event_end_is_dropped_then_the_dropped_waiter_is_still_woken()
 {
    let (instance, mut reader, sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, events) = instance.into_parts();
    let (wakes, counting) = counting_waker();
    let mut next = pin!(reader.next());
    let mut dropped = pin!(sender.dropped());
    assert!(poll_with(next.as_mut(), &panicking_waker()).is_pending());
    assert!(poll_with(dropped.as_mut(), &counting).is_pending());
    let outcome = catch_unwind(AssertUnwindSafe(|| drop(events)));
    assert!(raised(outcome.unwrap_err()));
    assert_eq!(wakes.count(), 1);
}
