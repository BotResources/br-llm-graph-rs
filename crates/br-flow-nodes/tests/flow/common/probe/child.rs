use std::sync::Arc;
use std::sync::atomic::Ordering;

use br_graph_contract::{CancelView, Event, EventSender, InputReader, Reason};

use super::watch::{Live, locked};
use super::{Mode, ProbeCause, Shared};
use crate::common::{Item, ItemUpdate};

pub(super) async fn child(
    shared: Arc<Shared>,
    live: Live,
    position: usize,
    mut reader: InputReader<Item>,
    sender: EventSender<Item>,
    cancel: CancelView,
) {
    let mut dropped = sender.dropped();
    let first = tokio::select! {
        biased;
        () = &mut dropped => None,
        input = reader.next() => input,
    };
    let event = match first {
        Some(input) => {
            let value = input.fields().value;
            locked(&shared.stats.inputs).push((sender.id(), value));
            shared.started.send(value).ok();
            decide(&shared, value, &cancel, &mut dropped).await
        }
        None => None,
    };
    drop(live);
    match event {
        Some(event) => {
            reader.close();
            let extra = reader.drain().len();
            shared.stats.extra_inputs.fetch_add(extra, Ordering::SeqCst);
            sender.finish(event);
        }
        None => drop(sender),
    }
    dropped.await;
    shared.dropped.send(position).ok();
}

async fn decide(
    shared: &Shared,
    value: u32,
    cancel: &CancelView,
    dropped: &mut br_graph_contract::Dropped,
) -> Option<Event<Item>> {
    let answer = || Event::Result(ItemUpdate::Set(value * 2));
    let gate = shared.gates.gate(value);
    match (shared.mode)(value) {
        Mode::Answer => Some(answer()),
        Mode::Gate => tokio::select! {
            biased;
            () = &mut *dropped => None,
            () = cancel.triggered() => Some(Event::Cancelled),
            () = gate.notified() => Some(answer()),
        },
        Mode::Silent => tokio::select! {
            biased;
            () = &mut *dropped => None,
            () = cancel.triggered() => Some(Event::Cancelled),
        },
        Mode::Fail => Some(Event::failed(ProbeCause)),
        Mode::Refuse => Some(Event::Refused(Reason::new("no thanks"))),
        Mode::CancelAlone => Some(Event::Cancelled),
        Mode::Vanish => None,
        Mode::Reject => Some(Event::Result(ItemUpdate::Reject)),
        Mode::Invalidate => Some(Event::Result(ItemUpdate::Invalidate)),
        Mode::Record => Some(Event::Result(ItemUpdate::Record("could not read".into()))),
        Mode::IgnoreCancel => tokio::select! {
            biased;
            () = &mut *dropped => None,
            () = gate.notified() => Some(answer()),
        },
        Mode::SlowCancel => tokio::select! {
            biased;
            () = &mut *dropped => None,
            () = cancel.triggered() => {
                gate.notified().await;
                Some(Event::Cancelled)
            }
        },
        Mode::TriggerThenAnswer => {
            if let Some(handle) = locked(&shared.outer).as_ref() {
                handle.trigger();
            }
            Some(answer())
        }
    }
}
