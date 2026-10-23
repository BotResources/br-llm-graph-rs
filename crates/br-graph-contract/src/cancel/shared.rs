use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::task::Waker;

use crate::waiters::Waiters;

pub(super) struct Shared {
    triggered: AtomicBool,
    inner: Mutex<Inner>,
}

struct Inner {
    triggered: bool,
    waiters: Waiters,
    children: Vec<Weak<Shared>>,
}

impl Shared {
    pub(super) fn new(triggered: bool) -> Arc<Self> {
        Arc::new(Self {
            triggered: AtomicBool::new(triggered),
            inner: Mutex::new(Inner {
                triggered,
                waiters: Waiters::default(),
                children: Vec::new(),
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn is_triggered(&self) -> bool {
        self.triggered.load(Ordering::Acquire)
    }

    pub(super) fn trigger(self: &Arc<Self>) {
        let mut pending = vec![Arc::clone(self)];
        while let Some(shared) = pending.pop() {
            let (wakers, children) = shared.fire();
            pending.extend(children.iter().filter_map(Weak::upgrade));
            for waker in wakers {
                waker.wake();
            }
        }
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

    pub(super) fn child(&self) -> Arc<Self> {
        let mut inner = self.lock();
        if inner.triggered {
            return Self::new(true);
        }
        inner.children.retain(|child| child.strong_count() > 0);
        let child = Self::new(false);
        inner.children.push(Arc::downgrade(&child));
        child
    }

    pub(super) fn poll_triggered(&self, key: &mut Option<u64>, waker: &Waker) -> bool {
        let mut inner = self.lock();
        if inner.triggered {
            return true;
        }
        inner.waiters.register(key, waker);
        false
    }

    pub(super) fn remove_waiter(&self, key: u64) {
        self.lock().waiters.remove(key);
    }

    #[cfg(test)]
    pub(super) fn waiter_count(&self) -> usize {
        self.lock().waiters.len()
    }

    #[cfg(test)]
    pub(super) fn child_slots(&self) -> usize {
        self.lock().children.len()
    }
}
