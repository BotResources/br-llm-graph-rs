use std::sync::{Arc, Barrier};
use std::time::Duration;

use br_graph_contract::{Event, Input, InputClosed, Instance, Reason};
use tokio::time::timeout;

use crate::common::{Tally, TallyUpdate, id, state};

fn input(total: i64) -> Input<Tally> {
    Input::new(Arc::new(state(total)))
}

async fn pending<T>(future: impl std::future::Future<Output = T>) -> bool {
    timeout(Duration::from_millis(50), future).await.is_err()
}

#[test]
fn given_an_instance_when_created_then_every_half_reads_the_same_id() {
    let (instance, reader, sender) = Instance::<Tally>::channel(id(1));
    let (input_end, event_end) = instance.into_parts();
    assert_eq!(input_end.id(), id(1));
    assert_eq!(event_end.id(), id(1));
    assert_eq!(reader.id(), id(1));
    assert_eq!(sender.id(), id(1));
}

#[tokio::test]
async fn given_inputs_given_in_order_when_read_then_they_come_out_in_order() {
    let (instance, mut reader, _sender) = Instance::<Tally>::channel(id(1));
    let (input_end, _events) = instance.into_parts();
    for total in 0..5 {
        input_end.give(input(total)).unwrap();
    }
    for total in 0..5 {
        assert_eq!(reader.next().await.unwrap().fields().total, total);
    }
}

#[tokio::test]
async fn given_no_reader_when_many_inputs_are_given_then_give_never_waits_and_accepts_all() {
    let (instance, _reader, _sender) = Instance::<Tally>::channel(id(1));
    let (input_end, _events) = instance.into_parts();
    for total in 0..10_000 {
        assert!(input_end.give(input(total)).is_ok());
    }
}

#[tokio::test]
async fn given_an_input_when_given_then_it_holds_the_state_of_the_hand_over() {
    let (instance, mut reader, _sender) = Instance::<Tally>::channel(id(1));
    let (input_end, _events) = instance.into_parts();
    let first = Arc::new(state(1));
    input_end.give(Input::new(Arc::clone(&first))).unwrap();
    let later = first.apply(TallyUpdate::Add(10)).unwrap();
    input_end.give(Input::new(Arc::new(later))).unwrap();
    assert_eq!(reader.next().await.unwrap().fields().total, 1);
    assert_eq!(reader.next().await.unwrap().state().fields().total, 11);
}

#[tokio::test]
async fn given_an_input_when_given_then_no_event_comes_back_for_it() {
    let (instance, _reader, _sender) = Instance::<Tally>::channel(id(1));
    let (input_end, mut events) = instance.into_parts();
    input_end.give(input(1)).unwrap();
    assert!(pending(events.recv()).await);
}

#[tokio::test]
async fn given_a_silent_instance_when_waiting_for_an_event_then_silence_is_not_an_event() {
    let (instance, _reader, _sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, mut events) = instance.into_parts();
    assert!(pending(events.recv()).await);
}

#[tokio::test]
async fn given_a_pending_next_when_an_input_arrives_from_another_task_then_it_wakes() {
    let (instance, mut reader, _sender) = Instance::<Tally>::channel(id(1));
    let (input_end, _events) = instance.into_parts();
    let waiting = tokio::spawn(async move { reader.next().await.map(|i| i.fields().total) });
    tokio::time::sleep(Duration::from_millis(20)).await;
    input_end.give(input(5)).unwrap();
    assert_eq!(waiting.await.unwrap(), Some(5));
}

#[tokio::test]
async fn given_a_pending_next_when_the_input_is_closed_then_it_ends_with_none() {
    let (instance, mut reader, sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, _events) = instance.into_parts();
    let waiting = tokio::spawn(async move { reader.next().await.is_none() });
    tokio::time::sleep(Duration::from_millis(20)).await;
    sender.close_input();
    assert!(waiting.await.unwrap());
}

#[tokio::test]
async fn given_accepted_inputs_when_the_input_is_closed_then_they_are_still_read_then_none() {
    let (instance, mut reader, _sender) = Instance::<Tally>::channel(id(1));
    let (input_end, _events) = instance.into_parts();
    input_end.give(input(1)).unwrap();
    input_end.give(input(2)).unwrap();
    reader.close();
    reader.close();
    assert_eq!(input_end.give(input(3)), Err(InputClosed));
    assert_eq!(reader.next().await.unwrap().fields().total, 1);
    assert_eq!(reader.next().await.unwrap().fields().total, 2);
    assert!(reader.next().await.is_none());
    assert!(reader.next().await.is_none());
}

#[tokio::test]
async fn given_a_closed_input_when_drained_then_every_accepted_input_comes_in_order() {
    let (instance, mut reader, _sender) = Instance::<Tally>::channel(id(1));
    let (input_end, _events) = instance.into_parts();
    for total in 0..4 {
        input_end.give(input(total)).unwrap();
    }
    reader.close();
    let drained: Vec<i64> = reader
        .drain()
        .iter()
        .map(|input| input.fields().total)
        .collect();
    assert_eq!(drained, vec![0, 1, 2, 3]);
    assert!(reader.drain().is_empty());
}

#[test]
fn given_a_race_of_gives_and_a_close_when_read_then_every_accepted_input_is_read_and_no_other() {
    let mut rounds_with_both_outcomes = 0;
    for round in 0..300 {
        let (instance, mut reader, _sender) = Instance::<Tally>::channel(id(round));
        let (input_end, _events) = instance.into_parts();
        let input_end = Arc::new(input_end);
        let start = Arc::new(Barrier::new(5));
        let givers: Vec<_> = (0..4)
            .map(|_| {
                let input_end = Arc::clone(&input_end);
                let start = Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    let outcomes: Vec<bool> = (0..200)
                        .map(|total| input_end.give(input(total)).is_ok())
                        .collect();
                    outcomes
                })
            })
            .collect();
        start.wait();
        reader.close();
        let outcomes: Vec<bool> = givers
            .into_iter()
            .flat_map(|giver| giver.join().unwrap())
            .collect();
        let accepted = outcomes.iter().filter(|accepted| **accepted).count();
        assert_eq!(reader.drain().len(), accepted);
        if accepted > 0 && accepted < outcomes.len() {
            rounds_with_both_outcomes += 1;
        }
    }
    assert!(rounds_with_both_outcomes > 0);
}

#[tokio::test]
async fn given_a_result_when_the_instance_finishes_then_it_is_the_only_event_then_the_end_ends() {
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, mut events) = instance.into_parts();
    sender.finish(Event::Result(TallyUpdate::Add(2)));
    assert!(matches!(
        events.recv().await,
        Some(Event::Result(TallyUpdate::Add(2)))
    ));
    assert!(events.recv().await.is_none());
    assert!(events.recv().await.is_none());
}

#[tokio::test]
async fn given_each_event_kind_when_sent_then_it_is_received_unchanged() {
    let cases: Vec<Event<Tally>> = vec![
        Event::Failed("broken".into()),
        Event::Refused(Reason::new("second time")),
        Event::Cancelled,
    ];
    for event in cases {
        let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
        let (_input_end, mut events) = instance.into_parts();
        let expected = format!("{event:?}");
        sender.finish(event);
        let received = events.recv().await.unwrap();
        assert_eq!(format!("{received:?}"), expected);
        assert!(events.recv().await.is_none());
    }
}

#[tokio::test]
async fn given_a_pending_recv_when_the_instance_finishes_from_another_task_then_it_wakes() {
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, mut events) = instance.into_parts();
    let waiting = tokio::spawn(async move { events.recv().await.is_some() });
    tokio::time::sleep(Duration::from_millis(20)).await;
    sender.finish(Event::Cancelled);
    assert!(waiting.await.unwrap());
}

#[tokio::test]
async fn given_a_finished_instance_when_an_input_is_given_then_it_is_refused_as_input_closed() {
    let (instance, mut reader, sender) = Instance::<Tally>::channel(id(1));
    let (input_end, _events) = instance.into_parts();
    input_end.give(input(1)).unwrap();
    sender.finish(Event::Cancelled);
    assert_eq!(input_end.give(input(2)), Err(InputClosed));
    assert!(reader.next().await.is_none());
}

#[tokio::test]
async fn given_a_sender_dropped_without_event_when_received_then_the_end_ends_with_none() {
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let (input_end, mut events) = instance.into_parts();
    drop(sender);
    assert!(events.recv().await.is_none());
    assert_eq!(input_end.give(input(1)), Err(InputClosed));
}

#[tokio::test]
async fn given_a_reader_dropped_when_an_input_is_given_then_the_input_end_is_closed_but_the_event_still_comes()
 {
    let (instance, reader, sender) = Instance::<Tally>::channel(id(1));
    let (input_end, mut events) = instance.into_parts();
    drop(reader);
    assert_eq!(input_end.give(input(1)), Err(InputClosed));
    sender.finish(Event::Cancelled);
    assert!(matches!(events.recv().await, Some(Event::Cancelled)));
}

#[tokio::test]
async fn given_the_reader_and_the_sender_in_two_tasks_when_the_node_works_then_it_ends_with_one_event()
 {
    let (instance, mut reader, sender) = Instance::<Tally>::channel(id(1));
    let (input_end, mut events) = instance.into_parts();
    let reading = tokio::spawn(async move {
        let mut total = 0;
        while let Some(input) = reader.next().await {
            total += input.fields().total;
        }
        total
    });
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        sender.close_input();
        let total = reading.await.unwrap();
        sender.finish(Event::Result(TallyUpdate::Add(total)));
    });
    for total in 1..=4 {
        input_end.give(input(total)).unwrap();
    }
    assert!(matches!(
        events.recv().await,
        Some(Event::Result(TallyUpdate::Add(10)))
    ));
    assert!(events.recv().await.is_none());
    assert_eq!(input_end.give(input(5)), Err(InputClosed));
}
