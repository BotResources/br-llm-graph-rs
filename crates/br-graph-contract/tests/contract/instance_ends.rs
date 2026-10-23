use std::sync::Arc;
use std::time::Duration;

use br_graph_contract::{Event, Input, InputClosed, Instance};
use tokio::time::timeout;

use crate::common::{Tally, TallyUpdate, id, state};

fn input(total: i64) -> Input<Tally> {
    Input::new(Arc::new(state(total)))
}

async fn resolves(future: impl std::future::Future<Output = ()>) -> bool {
    timeout(Duration::from_secs(2), future).await.is_ok()
}

#[tokio::test]
async fn given_a_live_instance_when_asked_then_it_is_not_dropped_and_dropped_stays_pending() {
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, _events) = instance.into_parts();
    assert!(!sender.is_dropped());
    assert!(
        timeout(Duration::from_millis(50), sender.dropped())
            .await
            .is_err()
    );
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
    assert!(events.recv().await.is_none());
}

#[tokio::test]
async fn given_the_event_end_dropped_when_the_node_looks_then_the_instance_is_dropped_and_give_is_refused()
 {
    let (instance, mut reader, sender) = Instance::<Tally>::channel(id(1));
    let (input_end, events) = instance.into_parts();
    drop(events);
    assert!(resolves(sender.dropped()).await);
    assert_eq!(input_end.give(input(1)), Err(InputClosed));
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

#[tokio::test]
async fn given_a_pending_next_when_the_executor_drops_an_end_then_it_ends_with_none() {
    let (instance, mut reader, _sender) = Instance::<Tally>::channel(id(1));
    let (input_end, _events) = instance.into_parts();
    let waiting = tokio::spawn(async move { reader.next().await.is_none() });
    tokio::time::sleep(Duration::from_millis(20)).await;
    drop(input_end);
    assert!(waiting.await.unwrap());
}

#[tokio::test]
async fn given_a_pending_recv_when_the_input_end_is_dropped_then_it_ends_with_none() {
    let (instance, _reader, _sender) = Instance::<Tally>::channel(id(1));
    let (input_end, mut events) = instance.into_parts();
    let waiting = tokio::spawn(async move { events.recv().await.is_none() });
    tokio::time::sleep(Duration::from_millis(20)).await;
    drop(input_end);
    assert!(waiting.await.unwrap());
}

#[tokio::test]
async fn given_an_event_sent_before_the_input_end_is_dropped_when_received_then_it_is_delivered() {
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let (input_end, mut events) = instance.into_parts();
    sender.finish(Event::Cancelled);
    drop(input_end);
    assert!(matches!(events.recv().await, Some(Event::Cancelled)));
}

#[tokio::test]
async fn given_many_waiters_on_dropped_when_the_instance_is_dropped_then_all_wake() {
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let waiters: Vec<_> = (0..50).map(|_| tokio::spawn(sender.dropped())).collect();
    tokio::time::sleep(Duration::from_millis(20)).await;
    drop(instance);
    for waiter in waiters {
        assert!(resolves(async { waiter.await.unwrap() }).await);
    }
}
