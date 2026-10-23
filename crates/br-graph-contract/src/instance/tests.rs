#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::pin::pin;
use std::task::{Context, Poll, Waker};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Instance;
use crate::{Fields, InstanceId, Reason};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Nothing {}

impl Fields for Nothing {
    type Update = ();

    fn empty_update() {}

    fn validate(&self) -> Result<(), Reason> {
        Ok(())
    }

    fn apply(&mut self, _update: ()) -> Result<(), Reason> {
        Ok(())
    }
}

fn id() -> InstanceId {
    InstanceId::new(Uuid::from_u128((0x7_u128 << 76) | (0b10_u128 << 62) | 1)).unwrap()
}

#[tokio::test]
async fn given_dropped_raced_in_a_select_loop_100k_times_when_inspected_then_no_slot_is_left() {
    let (instance, mut reader, sender) = Instance::<Nothing>::channel(id());
    let (input_end, events) = instance.into_parts();
    for _ in 0..100_000 {
        tokio::select! {
            () = sender.dropped() => unreachable!(),
            () = tokio::task::yield_now() => {}
        }
    }
    assert_eq!(input_end.core.waiter_slots(), 0);
    drop(events);
    drop(reader.drain());
}

#[tokio::test]
async fn given_next_raced_in_a_select_loop_100k_times_when_inspected_then_no_slot_is_left() {
    let (instance, mut reader, _sender) = Instance::<Nothing>::channel(id());
    let (input_end, _events) = instance.into_parts();
    for _ in 0..100_000 {
        tokio::select! {
            _ = reader.next() => unreachable!(),
            () = tokio::task::yield_now() => {}
        }
    }
    assert_eq!(input_end.core.waiter_slots(), 0);
}

#[tokio::test]
async fn given_recv_raced_in_a_select_loop_100k_times_when_inspected_then_no_slot_is_left() {
    let (instance, _reader, _sender) = Instance::<Nothing>::channel(id());
    let (input_end, mut events) = instance.into_parts();
    for _ in 0..100_000 {
        tokio::select! {
            _ = events.recv() => unreachable!(),
            () = tokio::task::yield_now() => {}
        }
    }
    assert_eq!(input_end.core.waiter_slots(), 0);
}

#[test]
fn given_one_dropped_future_polled_many_times_when_inspected_then_it_holds_one_slot() {
    let (instance, _reader, sender) = Instance::<Nothing>::channel(id());
    let (input_end, _events) = instance.into_parts();
    {
        let mut future = pin!(sender.dropped());
        for _ in 0..1_000 {
            let polled = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()));
            assert_eq!(polled, Poll::Pending);
        }
        assert_eq!(input_end.core.waiter_slots(), 1);
    }
    assert_eq!(input_end.core.waiter_slots(), 0);
}
