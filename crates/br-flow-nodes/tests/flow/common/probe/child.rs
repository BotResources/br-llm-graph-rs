use std::sync::Arc;
use std::sync::atomic::Ordering;

use br_graph_contract::{CancelView, Dropped, Event, EventSender, InputReader, Reason};

use super::watch::{Live, locked};
use super::{Mark, Mode, ProbeCause, Shared, Witness};
use crate::common::{Hook, Item, ItemUpdate};

pub(super) async fn child(
    shared: Arc<Shared>,
    live: Live,
    position: usize,
    mut reader: InputReader<Item>,
    sender: EventSender<Item>,
    cancel: CancelView,
) {
    let mut dropped = Witness::new(sender.dropped(), &shared.log, Mark::Dropped(position));
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
            decide(&shared, position, value, &cancel, &mut dropped).await
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
    position: usize,
    value: u32,
    cancel: &CancelView,
    dropped: &mut Witness<Dropped>,
) -> Option<Event<Item>> {
    let answer = || Event::Result(ItemUpdate::Set(value * 2));
    let gate = shared.gates.gate(value);
    let triggered = || Witness::new(cancel.triggered(), &shared.log, Mark::Triggered(position));
    match (shared.mode)(value) {
        Mode::Answer => Some(answer()),
        Mode::Gate => tokio::select! {
            biased;
            () = &mut *dropped => None,
            () = triggered() => Some(Event::Cancelled),
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
        Mode::PanicOnApply => Some(Event::Result(ItemUpdate::Explode)),
        Mode::PanicWork => panic!("the work of the instance exploded"),
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
        Mode::TriggerOnApply => match locked(&shared.outer).as_ref() {
            Some(handle) => Some(Event::Result(ItemUpdate::Trigger(Hook(handle.clone())))),
            None => Some(answer()),
        },
        Mode::TriggerThenFail => {
            if let Some(handle) = locked(&shared.outer).as_ref() {
                handle.trigger();
            }
            Some(Event::failed(ProbeCause))
        }
        Mode::TriggerThenAnswer => {
            if let Some(handle) = locked(&shared.outer).as_ref() {
                handle.trigger();
            }
            Some(answer())
        }
    }
}
