use std::collections::HashMap;
use std::task::Waker;

#[derive(Default)]
pub(crate) struct Waiters {
    next_key: u64,
    slots: HashMap<u64, Waker>,
}

impl Waiters {
    #[must_use = "the displaced waker must be dropped after the lock is released"]
    pub(crate) fn register(&mut self, key: &mut Option<u64>, waker: &Waker) -> Option<Waker> {
        let Some(existing) = *key else {
            let fresh = self.next_key;
            self.next_key = self.next_key.wrapping_add(1);
            self.slots.insert(fresh, waker.clone());
            *key = Some(fresh);
            return None;
        };
        match self.slots.get_mut(&existing) {
            Some(slot) if slot.will_wake(waker) => None,
            Some(slot) => Some(std::mem::replace(slot, waker.clone())),
            None => {
                self.slots.insert(existing, waker.clone());
                None
            }
        }
    }

    #[must_use = "the removed waker must be dropped after the lock is released"]
    pub(crate) fn remove(&mut self, key: u64) -> Option<Waker> {
        self.slots.remove(&key)
    }

    #[must_use = "the drained wakers must be woken or dropped after the lock is released"]
    pub(crate) fn drain(&mut self) -> Vec<Waker> {
        self.slots.drain().map(|(_, waker)| waker).collect()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::task::{Wake, Waker};

    use super::Waiters;

    struct Noop;

    impl Wake for Noop {
        fn wake(self: Arc<Self>) {
            let _ = self;
        }
    }

    struct CountedDrop(Arc<AtomicUsize>);

    impl Wake for CountedDrop {
        fn wake(self: Arc<Self>) {
            let _ = self;
        }
    }

    impl Drop for CountedDrop {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn given_a_registered_waker_when_replaced_then_the_old_one_is_handed_back_alive() {
        let drops = Arc::new(AtomicUsize::new(0));
        let first = Waker::from(Arc::new(CountedDrop(Arc::clone(&drops))));
        let second = Waker::from(Arc::new(Noop));
        let mut waiters = Waiters::default();
        let mut key = None;
        assert!(waiters.register(&mut key, &first).is_none());
        drop(first);
        let displaced = waiters.register(&mut key, &second);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        assert!(displaced.is_some());
        drop(displaced);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert_eq!(waiters.len(), 1);
    }

    #[test]
    fn given_the_same_waker_when_registered_again_then_nothing_is_displaced() {
        let waker = Waker::from(Arc::new(Noop));
        let mut waiters = Waiters::default();
        let mut key = None;
        assert!(waiters.register(&mut key, &waker).is_none());
        assert!(waiters.register(&mut key, &waker).is_none());
        assert_eq!(waiters.len(), 1);
    }

    #[test]
    fn given_a_registered_waker_when_removed_then_it_is_handed_back_alive() {
        let drops = Arc::new(AtomicUsize::new(0));
        let waker = Waker::from(Arc::new(CountedDrop(Arc::clone(&drops))));
        let mut waiters = Waiters::default();
        let mut key = None;
        assert!(waiters.register(&mut key, &waker).is_none());
        drop(waker);
        let Some(registered) = key else {
            return;
        };
        let removed = waiters.remove(registered);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        drop(removed);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert_eq!(waiters.len(), 0);
    }
}
