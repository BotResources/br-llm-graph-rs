use std::collections::VecDeque;
use std::mem;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

use super::lifecycle::{Executor, Gate, NodeSide, Outside, remember};
use super::{Event, EventEndError, Input, InputClosed};
use crate::waiters::{Waiters, wake_all};
use crate::{Fields, InstanceId};

pub(super) struct Core<F: Fields> {
    id: InstanceId,
    inner: Mutex<Inner<F>>,
}

struct Inner<F: Fields> {
    queue: VecDeque<Input<F>>,
    node: NodeSide<F>,
    executor: Executor,
    reader_waker: Option<Waker>,
    event_waker: Option<Waker>,
    dropped_waiters: Waiters,
}

impl<F: Fields> Inner<F> {
    fn accepts_input(&self) -> bool {
        matches!(self.node, NodeSide::Working(Gate::Open))
    }

    fn retire(&mut self, outside: &mut Outside<F>) {
        outside.inputs = mem::take(&mut self.queue);
        outside.wakers.extend(self.reader_waker.take());
        outside.wakers.extend(self.event_waker.take());
    }
}

impl<F: Fields> Core<F> {
    pub(super) fn new(id: InstanceId) -> Self {
        Self {
            id,
            inner: Mutex::new(Inner {
                queue: VecDeque::new(),
                node: NodeSide::Working(Gate::Open),
                executor: Executor::Attached,
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
            if !inner.accepts_input() {
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
            match &mut inner.node {
                NodeSide::Working(gate @ Gate::Open) => {
                    *gate = Gate::Closed;
                    inner.reader_waker.take()
                }
                _ => None,
            }
        };
        wake_all(waker);
    }

    pub(super) fn poll_next(&self, cx: &mut Context<'_>) -> Poll<Option<Input<F>>> {
        let owned = cx.waker().clone();
        let mut inner = self.lock();
        if let Some(input) = inner.queue.pop_front() {
            return Poll::Ready(Some(input));
        }
        if !inner.accepts_input() {
            return Poll::Ready(None);
        }
        let replaced = remember(&mut inner.reader_waker, owned);
        drop(inner);
        drop(replaced);
        Poll::Pending
    }

    pub(super) fn forget_reader_waker(&self) {
        let forgotten = self.lock().reader_waker.take();
        drop(forgotten);
    }

    pub(super) fn drain(&self) -> Vec<Input<F>> {
        let drained = mem::take(&mut self.lock().queue);
        drained.into()
    }

    pub(super) fn finish(&self, event: Event<F>) {
        let mut outside = Outside::default();
        {
            let mut inner = self.lock();
            if matches!(inner.node, NodeSide::Working(_)) {
                inner.node = NodeSide::Sent(event);
            } else {
                outside.event = Some(event);
            }
            inner.retire(&mut outside);
        }
        outside.settle();
    }

    pub(super) fn sender_gone(&self) {
        let mut outside = Outside::default();
        {
            let mut inner = self.lock();
            if matches!(inner.node, NodeSide::Working(_)) {
                inner.node = NodeSide::Vanished;
            }
            inner.retire(&mut outside);
        }
        outside.settle();
    }

    pub(super) fn input_end_dropped(&self) {
        self.detach(false);
    }

    pub(super) fn event_end_dropped(&self) {
        self.detach(true);
    }

    fn detach(&self, discard_event: bool) {
        let mut outside = Outside::default();
        {
            let mut inner = self.lock();
            inner.executor = Executor::Dropped;
            match mem::replace(&mut inner.node, NodeSide::Ended) {
                NodeSide::Sent(event) if discard_event => outside.event = Some(event),
                NodeSide::Sent(event) => inner.node = NodeSide::Sent(event),
                NodeSide::Vanished => inner.node = NodeSide::Vanished,
                NodeSide::Working(_) | NodeSide::Ended => {}
            }
            inner.retire(&mut outside);
            outside.wakers.extend(inner.dropped_waiters.drain());
        }
        outside.settle();
    }

    pub(super) fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Result<Event<F>, EventEndError>> {
        let owned = cx.waker().clone();
        let mut inner = self.lock();
        match mem::replace(&mut inner.node, NodeSide::Ended) {
            NodeSide::Sent(event) => Poll::Ready(Ok(event)),
            NodeSide::Ended => Poll::Ready(Err(EventEndError::Finished)),
            NodeSide::Vanished => {
                inner.node = NodeSide::Vanished;
                Poll::Ready(Err(EventEndError::Vanished))
            }
            NodeSide::Working(gate) => {
                inner.node = NodeSide::Working(gate);
                let replaced = remember(&mut inner.event_waker, owned);
                drop(inner);
                drop(replaced);
                Poll::Pending
            }
        }
    }

    pub(super) fn forget_event_waker(&self) {
        let forgotten = self.lock().event_waker.take();
        drop(forgotten);
    }

    pub(super) fn is_dropped(&self) -> bool {
        self.lock().executor == Executor::Dropped
    }

    pub(super) fn poll_dropped(&self, key: &mut Option<u64>, waker: &Waker) -> bool {
        let owned = waker.clone();
        let mut inner = self.lock();
        if inner.executor == Executor::Dropped {
            return true;
        }
        let replaced = inner.dropped_waiters.register(key, owned);
        drop(inner);
        drop(replaced);
        false
    }

    pub(super) fn remove_dropped_waiter(&self, key: u64) {
        let removed = self.lock().dropped_waiters.remove(key);
        drop(removed);
    }

    #[cfg(test)]
    pub(super) fn waiter_slots(&self) -> usize {
        let inner = self.lock();
        inner.dropped_waiters.len()
            + usize::from(inner.reader_waker.is_some())
            + usize::from(inner.event_waker.is_some())
    }
}
