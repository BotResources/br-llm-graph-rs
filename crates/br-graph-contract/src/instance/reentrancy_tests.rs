#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::pin::pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::task::{Context, Wake, Waker};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::core::Core;
use super::{Event, Input, Instance};
use crate::{Fields, InstanceId, Reason, State};

struct Trap(Mutex<Option<Box<dyn FnOnce() + Send>>>);

impl Trap {
    fn arm(action: impl FnOnce() + Send + 'static) -> Arc<Self> {
        Arc::new(Self(Mutex::new(Some(Box::new(action)))))
    }
}

impl Drop for Trap {
    fn drop(&mut self) {
        let action = self.0.lock().unwrap().take();
        if let Some(action) = action {
            action();
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Hooked {
    #[serde(skip)]
    _trap: Option<Arc<Trap>>,
}

impl PartialEq for Hooked {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Fields for Hooked {
    type Update = Option<Arc<Trap>>;

    fn empty_update() -> Self::Update {
        None
    }

    fn validate(&self) -> Result<(), Reason> {
        Ok(())
    }

    fn apply(&mut self, _update: Self::Update) -> Result<(), Reason> {
        Ok(())
    }
}

fn id() -> InstanceId {
    InstanceId::new(Uuid::from_u128((0x7_u128 << 76) | (0b10_u128 << 62) | 1)).unwrap()
}

fn within_a_few_seconds(job: impl FnOnce() + Send + 'static) {
    let (done, finished) = mpsc::channel();
    std::thread::spawn(move || {
        job();
        let _ = done.send(());
    });
    assert!(
        finished.recv_timeout(Duration::from_secs(5)).is_ok(),
        "the instance deadlocked while a user value was dropped"
    );
}

fn count_core_calls(core: &Arc<Core<Hooked>>, fired: &Arc<AtomicUsize>) -> Arc<Trap> {
    let core = Arc::clone(core);
    let fired = Arc::clone(fired);
    Trap::arm(move || {
        let _ = core.is_dropped();
        fired.fetch_add(1, Ordering::SeqCst);
    })
}

fn input_holding(trap: Arc<Trap>) -> Input<Hooked> {
    let state = State::create(Hooked { _trap: Some(trap) }).unwrap();
    Input::new(Arc::new(state))
}

struct Reentering {
    core: Arc<Core<Hooked>>,
    fired: Arc<AtomicUsize>,
}

impl Wake for Reentering {
    fn wake(self: Arc<Self>) {
        let _ = self.core.is_dropped();
    }
}

impl Drop for Reentering {
    fn drop(&mut self) {
        let _ = self.core.is_dropped();
        self.fired.fetch_add(1, Ordering::SeqCst);
    }
}

fn reentering_waker(core: &Arc<Core<Hooked>>, fired: &Arc<AtomicUsize>) -> Waker {
    Waker::from(Arc::new(Reentering {
        core: Arc::clone(core),
        fired: Arc::clone(fired),
    }))
}

#[test]
fn given_queued_inputs_whose_drop_reenters_when_the_executor_drops_an_end_then_nothing_deadlocks() {
    within_a_few_seconds(|| {
        for drop_event_end in [false, true] {
            let (instance, _reader, _sender) = Instance::<Hooked>::channel(id());
            let (input_end, events) = instance.into_parts();
            let fired = Arc::new(AtomicUsize::new(0));
            let trap = count_core_calls(&input_end.core, &fired);
            input_end.give(input_holding(trap)).unwrap();
            if drop_event_end {
                drop(events);
            } else {
                drop(input_end);
            }
            assert_eq!(fired.load(Ordering::SeqCst), 1);
        }
    });
}

#[test]
fn given_queued_inputs_whose_drop_reenters_when_the_node_finishes_then_nothing_deadlocks() {
    within_a_few_seconds(|| {
        let (instance, _reader, sender) = Instance::<Hooked>::channel(id());
        let (input_end, _events) = instance.into_parts();
        let fired = Arc::new(AtomicUsize::new(0));
        let trap = count_core_calls(&input_end.core, &fired);
        input_end.give(input_holding(trap)).unwrap();
        sender.finish(Event::Cancelled);
        assert_eq!(fired.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn given_queued_inputs_whose_drop_reenters_when_the_sender_is_dropped_then_nothing_deadlocks() {
    within_a_few_seconds(|| {
        let (instance, _reader, sender) = Instance::<Hooked>::channel(id());
        let (input_end, _events) = instance.into_parts();
        let fired = Arc::new(AtomicUsize::new(0));
        let trap = count_core_calls(&input_end.core, &fired);
        input_end.give(input_holding(trap)).unwrap();
        drop(sender);
        assert_eq!(fired.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn given_an_event_whose_drop_reenters_when_it_is_discarded_then_nothing_deadlocks() {
    within_a_few_seconds(|| {
        let (instance, _reader, sender) = Instance::<Hooked>::channel(id());
        let (input_end, _events) = instance.into_parts();
        let fired = Arc::new(AtomicUsize::new(0));
        let core = Arc::clone(&input_end.core);
        drop(input_end);
        sender.finish(Event::Result(Some(count_core_calls(&core, &fired))));
        assert_eq!(fired.load(Ordering::SeqCst), 1);
    });
    within_a_few_seconds(|| {
        let (instance, _reader, sender) = Instance::<Hooked>::channel(id());
        let (input_end, events) = instance.into_parts();
        let fired = Arc::new(AtomicUsize::new(0));
        sender.finish(Event::Result(Some(count_core_calls(
            &input_end.core,
            &fired,
        ))));
        assert_eq!(fired.load(Ordering::SeqCst), 0);
        drop(events);
        assert_eq!(fired.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn given_a_waker_whose_drop_reenters_when_it_is_replaced_or_forgotten_then_nothing_deadlocks() {
    within_a_few_seconds(|| {
        let (instance, mut reader, sender) = Instance::<Hooked>::channel(id());
        let (input_end, mut events) = instance.into_parts();
        let core = Arc::clone(&input_end.core);
        let fired = Arc::new(AtomicUsize::new(0));

        let (first, second) = (
            reentering_waker(&core, &fired),
            reentering_waker(&core, &fired),
        );
        {
            let mut next = pin!(reader.next());
            assert!(
                next.as_mut()
                    .poll(&mut Context::from_waker(&first))
                    .is_pending()
            );
            drop(first);
            assert!(
                next.as_mut()
                    .poll(&mut Context::from_waker(&second))
                    .is_pending()
            );
            assert_eq!(fired.load(Ordering::SeqCst), 1);
            drop(second);
        }
        assert_eq!(fired.load(Ordering::SeqCst), 2);

        let (third, fourth) = (
            reentering_waker(&core, &fired),
            reentering_waker(&core, &fired),
        );
        {
            let mut recv = pin!(events.recv());
            assert!(
                recv.as_mut()
                    .poll(&mut Context::from_waker(&third))
                    .is_pending()
            );
            drop(third);
            assert!(
                recv.as_mut()
                    .poll(&mut Context::from_waker(&fourth))
                    .is_pending()
            );
            assert_eq!(fired.load(Ordering::SeqCst), 3);
            drop(fourth);
        }
        assert_eq!(fired.load(Ordering::SeqCst), 4);

        let (fifth, sixth) = (
            reentering_waker(&core, &fired),
            reentering_waker(&core, &fired),
        );
        {
            let mut dropped = pin!(sender.dropped());
            assert!(
                dropped
                    .as_mut()
                    .poll(&mut Context::from_waker(&fifth))
                    .is_pending()
            );
            drop(fifth);
            assert!(
                dropped
                    .as_mut()
                    .poll(&mut Context::from_waker(&sixth))
                    .is_pending()
            );
            assert_eq!(fired.load(Ordering::SeqCst), 5);
            drop(sixth);
        }
        assert_eq!(fired.load(Ordering::SeqCst), 6);
    });
}
