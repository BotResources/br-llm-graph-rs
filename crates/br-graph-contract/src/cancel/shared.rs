use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::task::Waker;

use crate::waiters::{Waiters, wake_all};

const PRUNE_SLACK: usize = 16;

pub(super) struct Shared {
    triggered: AtomicBool,
    live_children: AtomicUsize,
    parent: Option<Arc<Shared>>,
    inner: Mutex<Inner>,
}

struct Inner {
    triggered: bool,
    waiters: Waiters,
    children: Vec<Weak<Shared>>,
}

impl Shared {
    pub(super) fn new(triggered: bool) -> Arc<Self> {
        Arc::new(Self::build(triggered, None))
    }

    fn build(triggered: bool, parent: Option<Arc<Shared>>) -> Self {
        Self {
            triggered: AtomicBool::new(triggered),
            live_children: AtomicUsize::new(0),
            parent,
            inner: Mutex::new(Inner {
                triggered,
                waiters: Waiters::default(),
                children: Vec::new(),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn is_triggered(&self) -> bool {
        let mut current = self;
        loop {
            if current.triggered.load(Ordering::Acquire) {
                return true;
            }
            match &current.parent {
                Some(parent) => current = parent,
                None => return false,
            }
        }
    }

    pub(super) fn trigger(self: &Arc<Self>) {
        let mut wakers = Vec::new();
        let mut pending = vec![Arc::clone(self)];
        while let Some(shared) = pending.pop() {
            let (fired, children) = shared.fire();
            wakers.extend(fired);
            pending.extend(children.iter().filter_map(Weak::upgrade));
        }
        wake_all(wakers);
    }

    fn fire(&self) -> (Vec<Waker>, Vec<Weak<Shared>>) {
        let mut inner = self.lock();
        if inner.triggered {
            return (Vec::new(), Vec::new());
        }
        inner.triggered = true;
        self.triggered.store(true, Ordering::Release);
        (inner.waiters.drain(), std::mem::take(&mut inner.children))
    }

    pub(super) fn child(self: &Arc<Self>) -> Arc<Self> {
        let mut inner = self.lock();
        if inner.triggered {
            return Self::new(true);
        }
        let live = self.live_children.load(Ordering::Relaxed);
        if inner.children.len() >= live.saturating_mul(2).saturating_add(PRUNE_SLACK) {
            inner.children.retain(|child| child.strong_count() > 0);
        }
        let child = Arc::new(Self::build(false, Some(Arc::clone(self))));
        self.live_children.fetch_add(1, Ordering::Relaxed);
        inner.children.push(Arc::downgrade(&child));
        child
    }

    pub(super) fn poll_triggered(&self, key: &mut Option<u64>, waker: &Waker) -> bool {
        if self.is_triggered() {
            return true;
        }
        let owned = waker.clone();
        let (ready, leftover) = {
            let mut inner = self.lock();
            if self.is_triggered() {
                (true, Some(owned))
            } else {
                (false, inner.waiters.register(key, owned))
            }
        };
        drop(leftover);
        ready
    }

    pub(super) fn remove_waiter(&self, key: u64) {
        let removed = self.lock().waiters.remove(key);
        drop(removed);
    }

    fn release_parent(&mut self) -> Option<Arc<Shared>> {
        let parent = self.parent.take()?;
        parent.live_children.fetch_sub(1, Ordering::Relaxed);
        Some(parent)
    }

    #[cfg(test)]
    pub(super) fn waiter_count(&self) -> usize {
        self.lock().waiters.len()
    }

    #[cfg(test)]
    pub(super) fn child_slots(&self) -> usize {
        self.lock().children.len()
    }

    #[cfg(test)]
    pub(super) fn live_child_count(&self) -> usize {
        self.live_children.load(Ordering::Relaxed)
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        let mut next = self.release_parent();
        while let Some(parent) = next {
            next = Arc::into_inner(parent).and_then(|mut shared| shared.release_parent());
        }
    }
}
