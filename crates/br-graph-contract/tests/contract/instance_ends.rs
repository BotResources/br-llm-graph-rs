use std::pin::pin;
use std::task::Poll;
use std::time::Duration;

use br_graph_contract::{Event, EventEndError, InputClosed, Instance};
use tokio::time::timeout;

use crate::common::{Tally, TallyUpdate, id};
use crate::instance::input;
use crate::park::{counting_waker, poll_with};

async fn resolves(future: impl std::future::Future<Output = ()>) -> bool {
    timeout(Duration::from_secs(2), future).await.is_ok()
}

#[test]
fn given_a_live_instance_when_asked_then_it_is_not_dropped_and_dropped_stays_pending() {
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, _events) = instance.into_parts();
    assert!(!sender.is_dropped());
    let (wakes, waker) = counting_waker();
    let mut dropped = pin!(sender.dropped());
    assert!(poll_with(dropped.as_mut(), &waker).is_pending());
    assert_eq!(wakes.count(), 0);
}

#[tokio::test]
async fn given_the_input_end_dropped_when_the_node_looks_then_the_instance_is_dropped() {
    let (instance, mut reader, sender) = Instance::<Tally>::channel(id(1));
    let (input_end, mut events) = instance.into_parts();
    input_end.give(input(1)).unwrap();
    let waiting = tokio::spawn(sender.dropped());
    drop(input_end);
    assert!(resolves(async { waiting.await.unwrap() }).await);
    assert!(sender.is_dropped());
    assert!(reader.next().await.is_none());
    sender.finish(Event::Result(TallyUpdate::Nothing));
    assert!(matches!(events.recv().await, Err(EventEndError::Finished)));
}

#[tokio::test]
async fn given_the_event_end_dropped_when_the_node_looks_then_the_instance_is_dropped_and_give_is_refused()
 {
    let (instance, mut reader, sender) = Instance::<Tally>::channel(id(1));
    let (input_end, events) = instance.into_parts();
    input_end.give(input(1)).unwrap();
    drop(events);
    assert!(resolves(sender.dropped()).await);
    assert_eq!(input_end.give(input(2)), Err(InputClosed));
    assert!(reader.next().await.is_none());
}

#[tokio::test]
async fn given_the_whole_instance_dropped_when_the_node_looks_then_next_never_waits_for_ever() {
    let (instance, mut reader, sender) = Instance::<Tally>::channel(id(1));
    drop(instance);
    assert!(sender.is_dropped());
    assert!(
        timeout(Duration::from_secs(2), reader.next())
            .await
            .unwrap()
            .is_none()
    );
}

#[test]
fn given_a_pending_next_when_the_executor_drops_an_end_then_the_waker_fires_and_it_ends_with_none()
{
    let (instance, mut reader, _sender) = Instance::<Tally>::channel(id(1));
    let (input_end, _events) = instance.into_parts();
    let (wakes, waker) = counting_waker();
    let mut next = pin!(reader.next());
    assert!(poll_with(next.as_mut(), &waker).is_pending());
    drop(input_end);
    assert_eq!(wakes.count(), 1);
    assert!(matches!(
        poll_with(next.as_mut(), &waker),
        Poll::Ready(None)
    ));
}

#[test]
fn given_a_pending_recv_when_the_input_end_is_dropped_then_the_waker_fires_and_it_is_finished() {
    let (instance, _reader, _sender) = Instance::<Tally>::channel(id(1));
    let (input_end, mut events) = instance.into_parts();
    let (wakes, waker) = counting_waker();
    let mut recv = pin!(events.recv());
    assert!(poll_with(recv.as_mut(), &waker).is_pending());
    drop(input_end);
    assert_eq!(wakes.count(), 1);
    assert!(matches!(
        poll_with(recv.as_mut(), &waker),
        Poll::Ready(Err(EventEndError::Finished))
    ));
}

#[tokio::test]
async fn given_the_input_end_dropped_with_a_silent_sender_when_received_then_it_is_finished_every_time()
 {
    let (instance, _reader, _sender) = Instance::<Tally>::channel(id(1));
    let (input_end, mut events) = instance.into_parts();
    drop(input_end);
    assert!(matches!(events.recv().await, Err(EventEndError::Finished)));
    assert!(matches!(events.recv().await, Err(EventEndError::Finished)));
}

#[tokio::test]
async fn given_an_event_sent_before_the_input_end_is_dropped_when_received_then_it_is_delivered() {
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let (input_end, mut events) = instance.into_parts();
    sender.finish(Event::Cancelled);
    drop(input_end);
    assert!(matches!(events.recv().await, Ok(Event::Cancelled)));
    assert!(matches!(events.recv().await, Err(EventEndError::Finished)));
}

#[test]
fn given_many_waiters_on_dropped_when_the_instance_is_dropped_then_all_wake_and_resolve() {
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let mut waiting: Vec<_> = (0..50)
        .map(|_| {
            let (wakes, waker) = counting_waker();
            let mut future = Box::pin(sender.dropped());
            assert!(poll_with(future.as_mut(), &waker).is_pending());
            (wakes, waker, future)
        })
        .collect();
    drop(instance);
    for (wakes, waker, future) in &mut waiting {
        assert_eq!(wakes.count(), 1);
        assert!(poll_with(future.as_mut(), waker).is_ready());
    }
}
