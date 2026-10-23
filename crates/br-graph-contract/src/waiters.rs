use std::collections::HashMap;
use std::task::Waker;

#[derive(Default)]
pub(crate) struct Waiters {
    next_key: u64,
    slots: HashMap<u64, Waker>,
}

impl Waiters {
    pub(crate) fn register(&mut self, key: &mut Option<u64>, waker: &Waker) {
        let Some(existing) = *key else {
            let fresh = self.next_key;
            self.next_key = self.next_key.wrapping_add(1);
            self.slots.insert(fresh, waker.clone());
            *key = Some(fresh);
            return;
        };
        match self.slots.get_mut(&existing) {
            Some(slot) if slot.will_wake(waker) => {}
            Some(slot) => *slot = waker.clone(),
            None => {
                self.slots.insert(existing, waker.clone());
            }
        }
    }

    pub(crate) fn remove(&mut self, key: u64) {
        self.slots.remove(&key);
    }

    pub(crate) fn drain(&mut self) -> Vec<Waker> {
        self.slots.drain().map(|(_, waker)| waker).collect()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }
}
