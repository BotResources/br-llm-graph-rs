use std::collections::VecDeque;
use std::task::Waker;

use super::{Event, Input};
use crate::Fields;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Gate {
    Open,
    Closed,
}

pub(super) enum NodeSide<F: Fields> {
    Working(Gate),
    Sent(Event<F>),
    Ended,
    Vanished,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Executor {
    Attached,
    Dropped,
}

pub(super) fn remember(slot: &mut Option<Waker>, waker: &Waker) -> Option<Waker> {
    match slot {
        Some(existing) if existing.will_wake(waker) => None,
        _ => slot.replace(waker.clone()),
    }
}

pub(super) struct Outside<F: Fields> {
    pub(super) wakers: Vec<Waker>,
    pub(super) inputs: VecDeque<Input<F>>,
    pub(super) event: Option<Event<F>>,
}

impl<F: Fields> Default for Outside<F> {
    fn default() -> Self {
        Self {
            wakers: Vec::new(),
            inputs: VecDeque::new(),
            event: None,
        }
    }
}

impl<F: Fields> Outside<F> {
    pub(super) fn settle(self) {
        let Self {
            wakers,
            inputs,
            event,
        } = self;
        for waker in wakers {
            waker.wake();
        }
        drop(inputs);
        drop(event);
    }
}
