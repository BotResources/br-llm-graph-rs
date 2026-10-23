use std::future::Future;

use br_graph_contract::{
    CancelHandle, CancelToken, CancelView, Event, EventEnd, EventSender, Graph, Input, InputEnd,
    InputReader, Instance, State, Structure,
};

use crate::common::Tally;

fn send_sync<T: Send + Sync>() {}

fn send<T: Send>(_: &T) {}

#[test]
fn given_the_shared_types_when_checked_then_they_can_be_shared_and_sent_between_threads() {
    send_sync::<State<Tally>>();
    send_sync::<Input<Tally>>();
    send_sync::<Instance<Tally>>();
    send_sync::<InputEnd<Tally>>();
    send_sync::<EventEnd<Tally>>();
    send_sync::<InputReader<Tally>>();
    send_sync::<EventSender<Tally>>();
    send_sync::<Event<Tally>>();
    send_sync::<Graph<Tally>>();
    send_sync::<Structure>();
    send_sync::<CancelToken>();
    send_sync::<CancelHandle>();
    send_sync::<CancelView>();
}

#[test]
fn given_the_returned_futures_when_checked_then_they_can_move_to_another_task() {
    let token = CancelToken::new();
    send(&token.view().triggered());
    let (instance, mut reader, sender) = Instance::<Tally>::channel(crate::common::id(1));
    let (_input_end, mut events) = instance.into_parts();
    send(&sender.dropped());
    send(&reader.next());
    send(&events.recv());
    fn is_future<T: Future>(_: &T) {}
    is_future(&token.view().triggered());
}

fn owned<T: Send + 'static>(_: T) {}

#[test]
fn given_the_dropped_future_when_taken_by_value_then_it_is_send_and_static() {
    let (instance, _reader, sender) = Instance::<Tally>::channel(crate::common::id(1));
    let (_input_end, _events) = instance.into_parts();
    owned(sender.dropped());
}
