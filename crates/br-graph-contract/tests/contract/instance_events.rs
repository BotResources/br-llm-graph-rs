use std::pin::pin;
use std::task::Poll;

use br_graph_contract::{Event, EventEndError, InputClosed, Instance, Reason};

use crate::common::{Tally, TallyUpdate, id};
use crate::instance::input;
use crate::park::{counting_waker, poll_with};

#[test]
fn given_an_input_when_given_then_no_event_comes_back_for_it() {
    let (instance, _reader, _sender) = Instance::<Tally>::channel(id(1));
    let (input_end, mut events) = instance.into_parts();
    input_end.give(input(1)).unwrap();
    let (wakes, waker) = counting_waker();
    let mut recv = pin!(events.recv());
    assert!(poll_with(recv.as_mut(), &waker).is_pending());
    assert_eq!(wakes.count(), 0);
}

#[test]
fn given_a_silent_instance_when_waiting_for_an_event_then_silence_is_not_an_event() {
    let (instance, _reader, _sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, mut events) = instance.into_parts();
    let (wakes, waker) = counting_waker();
    let mut recv = pin!(events.recv());
    for _ in 0..3 {
        assert!(poll_with(recv.as_mut(), &waker).is_pending());
    }
    assert_eq!(wakes.count(), 0);
}

#[tokio::test]
async fn given_a_result_when_the_instance_finishes_then_it_is_the_only_event_then_the_end_is_finished()
 {
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, mut events) = instance.into_parts();
    sender.finish(Event::Result(TallyUpdate::Add(2)));
    assert!(matches!(
        events.recv().await,
        Ok(Event::Result(TallyUpdate::Add(2)))
    ));
    assert!(matches!(events.recv().await, Err(EventEndError::Finished)));
    assert!(matches!(events.recv().await, Err(EventEndError::Finished)));
}

#[tokio::test]
async fn given_each_event_kind_when_sent_then_it_is_received_as_the_same_variant() {
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, mut events) = instance.into_parts();
    sender.finish(Event::Failed("broken".into()));
    let Ok(Event::Failed(cause)) = events.recv().await else {
        unreachable!("a failure was sent");
    };
    assert_eq!(cause.to_string(), "broken");

    let (instance, _reader, sender) = Instance::<Tally>::channel(id(2));
    let (_input_end, mut events) = instance.into_parts();
    sender.finish(Event::Refused(Reason::new("second time")));
    let Ok(Event::Refused(reason)) = events.recv().await else {
        unreachable!("a refusal was sent");
    };
    assert_eq!(reason, Reason::new("second time"));

    let (instance, _reader, sender) = Instance::<Tally>::channel(id(3));
    let (_input_end, mut events) = instance.into_parts();
    sender.finish(Event::Cancelled);
    assert!(matches!(events.recv().await, Ok(Event::Cancelled)));
    assert!(matches!(events.recv().await, Err(EventEndError::Finished)));
}

#[test]
fn given_each_event_when_debug_printed_then_it_names_the_case_and_hides_the_update() {
    let failed = Event::<Tally>::Failed("out of memory".into());
    assert!(matches!(&failed, Event::Failed(cause) if cause.to_string() == "out of memory"));
    let refused = Event::<Tally>::Refused("second time".into());
    assert!(matches!(&refused, Event::Refused(reason) if reason.as_str() == "second time"));
    let owned = Event::<Tally>::Refused(String::from("once only").into());
    assert!(matches!(owned, Event::Refused(reason) if reason == Reason::new("once only")));
    assert_eq!(format!("{:?}", Event::<Tally>::Cancelled), "Cancelled");
    assert!(!format!("{:?}", Event::<Tally>::Result(TallyUpdate::Add(1))).contains("Add"));
}

#[test]
fn given_a_pending_recv_when_the_instance_finishes_then_the_waker_fires_and_the_next_poll_is_ready()
{
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, mut events) = instance.into_parts();
    let (wakes, waker) = counting_waker();
    let mut recv = pin!(events.recv());
    assert!(poll_with(recv.as_mut(), &waker).is_pending());
    sender.finish(Event::Cancelled);
    assert_eq!(wakes.count(), 1);
    assert!(matches!(
        poll_with(recv.as_mut(), &waker),
        Poll::Ready(Ok(Event::Cancelled))
    ));
}

#[test]
fn given_a_recv_future_dropped_after_the_event_was_sent_when_received_again_then_no_event_is_lost()
{
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, mut events) = instance.into_parts();
    let (wakes, waker) = counting_waker();
    {
        let mut recv = pin!(events.recv());
        assert!(poll_with(recv.as_mut(), &waker).is_pending());
        sender.finish(Event::Result(TallyUpdate::Add(9)));
        assert_eq!(wakes.count(), 1);
    }
    let mut again = pin!(events.recv());
    assert!(matches!(
        poll_with(again.as_mut(), &waker),
        Poll::Ready(Ok(Event::Result(TallyUpdate::Add(9))))
    ));
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
async fn given_a_sender_dropped_without_event_when_received_then_the_node_side_has_vanished() {
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let (input_end, mut events) = instance.into_parts();
    drop(sender);
    assert!(matches!(events.recv().await, Err(EventEndError::Vanished)));
    assert!(matches!(events.recv().await, Err(EventEndError::Vanished)));
    assert_eq!(input_end.give(input(1)), Err(InputClosed));
}

#[test]
fn given_a_pending_recv_when_the_sender_is_dropped_then_the_waker_fires_and_it_is_vanished() {
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, mut events) = instance.into_parts();
    let (wakes, waker) = counting_waker();
    let mut recv = pin!(events.recv());
    assert!(poll_with(recv.as_mut(), &waker).is_pending());
    drop(sender);
    assert_eq!(wakes.count(), 1);
    assert!(matches!(
        poll_with(recv.as_mut(), &waker),
        Poll::Ready(Err(EventEndError::Vanished))
    ));
}

#[tokio::test]
async fn given_a_sender_that_finished_when_it_is_dropped_after_then_the_event_is_not_vanished() {
    let (instance, _reader, sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, mut events) = instance.into_parts();
    sender.finish(Event::Cancelled);
    assert!(matches!(events.recv().await, Ok(Event::Cancelled)));
    assert!(matches!(events.recv().await, Err(EventEndError::Finished)));
}

#[tokio::test]
async fn given_a_reader_dropped_when_the_node_finishes_then_the_event_still_comes() {
    let (instance, reader, sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, mut events) = instance.into_parts();
    drop(reader);
    sender.finish(Event::Cancelled);
    assert!(matches!(events.recv().await, Ok(Event::Cancelled)));
}

#[tokio::test]
async fn given_the_reader_and_the_sender_in_two_tasks_when_the_node_works_then_it_ends_with_one_event()
 {
    let (instance, mut reader, sender) = Instance::<Tally>::channel(id(1));
    let (input_end, mut events) = instance.into_parts();
    let (all_given, given) = tokio::sync::oneshot::channel::<()>();
    let reading = tokio::spawn(async move {
        given.await.unwrap();
        reader.close();
        let mut total = 0;
        while let Some(input) = reader.next().await {
            total += input.fields().total;
        }
        total
    });
    tokio::spawn(async move {
        let total = reading.await.unwrap();
        sender.finish(Event::Result(TallyUpdate::Add(total)));
    });
    for total in 1..=4 {
        input_end.give(input(total)).unwrap();
    }
    all_given.send(()).unwrap();
    assert!(matches!(
        events.recv().await,
        Ok(Event::Result(TallyUpdate::Add(10)))
    ));
    assert!(matches!(events.recv().await, Err(EventEndError::Finished)));
    assert_eq!(input_end.give(input(5)), Err(InputClosed));
}

#[test]
fn given_the_event_end_errors_when_displayed_then_they_tell_the_two_cases_apart() {
    assert_ne!(
        EventEndError::Finished.to_string(),
        EventEndError::Vanished.to_string()
    );
    assert_ne!(EventEndError::Finished, EventEndError::Vanished);
}
