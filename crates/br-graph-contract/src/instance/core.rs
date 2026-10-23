use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

use super::{Event, Input, InputClosed};
use crate::waiters::Waiters;
use crate::{Fields, InstanceId};

pub(super) struct Core<F: Fields> {
    id: InstanceId,
    inner: Mutex<Inner<F>>,
}

struct Inner<F: Fields> {
    queue: VecDeque<Input<F>>,
    input_closed: bool,
    event: Option<Event<F>>,
    finished: bool,
    sender_gone: bool,
    dropped: bool,
    reader_waker: Option<Waker>,
    event_waker: Option<Waker>,
    dropped_waiters: Waiters,
}

fn remember(slot: &mut Option<Waker>, waker: &Waker) {
    match slot {
        Some(existing) if existing.will_wake(waker) => {}
        _ => *slot = Some(waker.clone()),
    }
}

fn wake_all(wakers: impl IntoIterator<Item = Waker>) {
    for waker in wakers {
        waker.wake();
    }
}

impl<F: Fields> Core<F> {
    pub(super) fn new(id: InstanceId) -> Self {
        Self {
            id,
            inner: Mutex::new(Inner {
                queue: VecDeque::new(),
                input_closed: false,
                event: None,
                finished: false,
                sender_gone: false,
                dropped: false,
                reader_waker: None,
                event_waker: None,
                dropped_waiters: Waiters::default(),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner<F>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn id(&self) -> InstanceId {
        self.id
    }

    pub(super) fn give(&self, input: Input<F>) -> Result<(), InputClosed> {
        let waker = {
            let mut inner = self.lock();
            if inner.input_closed || inner.dropped {
                return Err(InputClosed);
            }
            inner.queue.push_back(input);
            inner.reader_waker.take()
        };
        wake_all(waker);
        Ok(())
    }

    pub(super) fn close_input(&self) {
        let waker = {
            let mut inner = self.lock();
            inner.input_closed = true;
            inner.reader_waker.take()
        };
        wake_all(waker);
    }

    pub(super) fn poll_next(&self, cx: &mut Context<'_>) -> Poll<Option<Input<F>>> {
        let mut inner = self.lock();
        if inner.dropped {
            return Poll::Ready(None);
        }
        if let Some(input) = inner.queue.pop_front() {
            return Poll::Ready(Some(input));
        }
        if inner.input_closed {
            return Poll::Ready(None);
        }
        remember(&mut inner.reader_waker, cx.waker());
        Poll::Pending
    }

    pub(super) fn forget_reader_waker(&self) {
        self.lock().reader_waker = None;
    }

    pub(super) fn drain(&self) -> Vec<Input<F>> {
        let mut inner = self.lock();
        if inner.dropped {
            return Vec::new();
        }
        inner.queue.drain(..).collect()
    }

    pub(super) fn finish(&self, event: Event<F>) {
        let wakers = {
            let mut inner = self.lock();
            if !inner.dropped && !inner.finished {
                inner.event = Some(event);
            }
            inner.finished = true;
            inner.input_closed = true;
            inner.queue.clear();
            [inner.reader_waker.take(), inner.event_waker.take()]
        };
        wake_all(wakers.into_iter().flatten());
    }

    pub(super) fn sender_gone(&self) {
        let wakers = {
            let mut inner = self.lock();
            inner.sender_gone = true;
            inner.input_closed = true;
            inner.queue.clear();
            [inner.reader_waker.take(), inner.event_waker.take()]
        };
        wake_all(wakers.into_iter().flatten());
    }

    pub(super) fn executor_dropped(&self) {
        let wakers = {
            let mut inner = self.lock();
            inner.dropped = true;
            inner.input_closed = true;
            inner.queue.clear();
            let mut wakers = inner.dropped_waiters.drain();
            wakers.extend(inner.reader_waker.take());
            wakers.extend(inner.event_waker.take());
            wakers
        };
        wake_all(wakers);
    }

    pub(super) fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Option<Event<F>>> {
        let mut inner = self.lock();
        if let Some(event) = inner.event.take() {
            return Poll::Ready(Some(event));
        }
        if inner.finished || inner.sender_gone || inner.dropped {
            return Poll::Ready(None);
        }
        remember(&mut inner.event_waker, cx.waker());
        Poll::Pending
    }

    pub(super) fn forget_event_waker(&self) {
        self.lock().event_waker = None;
    }

    pub(super) fn is_dropped(&self) -> bool {
        self.lock().dropped
    }

    pub(super) fn poll_dropped(&self, key: &mut Option<u64>, waker: &Waker) -> bool {
        let mut inner = self.lock();
        if inner.dropped {
            return true;
        }
        inner.dropped_waiters.register(key, waker);
        false
    }

    pub(super) fn remove_dropped_waiter(&self, key: u64) {
        self.lock().dropped_waiters.remove(key);
    }

    #[cfg(test)]
    pub(super) fn waiter_slots(&self) -> usize {
        let inner = self.lock();
        inner.dropped_waiters.len()
            + usize::from(inner.reader_waker.is_some())
            + usize::from(inner.event_waker.is_some())
    }
}
