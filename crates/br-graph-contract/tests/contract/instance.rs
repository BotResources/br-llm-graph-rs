use std::pin::pin;
use std::sync::Arc;
use std::task::Poll;

use br_graph_contract::{Input, InputClosed, Instance};

use crate::common::{Tally, TallyUpdate, id, state};
use crate::park::{counting_waker, poll_with};

pub fn input(total: i64) -> Input<Tally> {
    Input::new(Arc::new(state(total)))
}

#[test]
fn given_an_instance_when_created_then_every_half_reads_the_same_id() {
    let (instance, reader, sender) = Instance::<Tally>::channel(id(1));
    assert_eq!(instance.id(), id(1));
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

#[test]
fn given_an_input_when_converted_into_its_state_then_the_state_is_the_shared_one() {
    let shared = Arc::new(state(4));
    let input = Input::from(Arc::clone(&shared));
    assert_eq!(input.fields().total, 4);
    let recovered = input.clone().into_state();
    assert!(Arc::ptr_eq(&recovered, &shared));
    assert!(Arc::ptr_eq(&input.into_state(), &shared));
}

#[test]
fn given_a_pending_next_when_an_input_is_given_then_the_waker_fires_once_and_the_next_poll_is_ready()
 {
    let (instance, mut reader, _sender) = Instance::<Tally>::channel(id(1));
    let (input_end, _events) = instance.into_parts();
    let (wakes, waker) = counting_waker();
    let mut next = pin!(reader.next());
    assert!(poll_with(next.as_mut(), &waker).is_pending());
    assert_eq!(wakes.count(), 0);
    input_end.give(input(5)).unwrap();
    assert_eq!(wakes.count(), 1);
    let Poll::Ready(Some(received)) = poll_with(next.as_mut(), &waker) else {
        unreachable!("the input was given");
    };
    assert_eq!(received.fields().total, 5);
}

#[test]
fn given_a_pending_next_when_the_input_is_closed_then_the_waker_fires_and_it_ends_with_none() {
    let (instance, mut reader, sender) = Instance::<Tally>::channel(id(1));
    let (_input_end, _events) = instance.into_parts();
    let (wakes, waker) = counting_waker();
    let mut next = pin!(reader.next());
    assert!(poll_with(next.as_mut(), &waker).is_pending());
    sender.close_input();
    assert_eq!(wakes.count(), 1);
    assert!(matches!(
        poll_with(next.as_mut(), &waker),
        Poll::Ready(None)
    ));
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

#[tokio::test]
async fn given_a_reader_dropped_when_an_input_is_given_then_the_input_end_is_closed() {
    let (instance, reader, _sender) = Instance::<Tally>::channel(id(1));
    let (input_end, _events) = instance.into_parts();
    drop(reader);
    assert_eq!(input_end.give(input(1)), Err(InputClosed));
}

#[tokio::test]
async fn given_a_reader_closed_early_when_the_node_goes_on_then_inputs_given_before_stay_readable()
{
    let (instance, mut reader, sender) = Instance::<Tally>::channel(id(1));
    let (input_end, _events) = instance.into_parts();
    input_end.give(input(1)).unwrap();
    sender.close_input();
    assert_eq!(input_end.give(input(2)), Err(InputClosed));
    assert_eq!(reader.next().await.unwrap().fields().total, 1);
}
