use std::sync::{Mutex, PoisonError};
use std::task::Waker;

#[derive(Default)]
pub(super) struct Wakers {
    waiting: Mutex<Vec<Waker>>,
}

impl Wakers {
    pub(super) fn register(&self, waker: &Waker) {
        let mut waiting = self.waiting.lock().unwrap_or_else(PoisonError::into_inner);
        if !waiting.iter().any(|known| known.will_wake(waker)) {
            waiting.push(waker.clone());
        }
    }

    pub(super) fn wake_all(&self) {
        let woken =
            std::mem::take(&mut *self.waiting.lock().unwrap_or_else(PoisonError::into_inner));
        for waker in woken {
            waker.wake();
        }
    }
}
