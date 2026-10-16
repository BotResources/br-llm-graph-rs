//! What a part of the library keeps for the length of one run, and no
//! longer: a slot per type, empty when the run starts, dropped with it.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

type Slots = HashMap<TypeId, Arc<dyn Any + Send + Sync>>;

/// The slots of one run, shared by every context derived for its nodes and
/// map items. A nested run (a call of another graph) has its own: nothing
/// kept here outlives its run or crosses into another, whatever the run ids
/// the host gives.
#[derive(Clone, Default)]
pub(crate) struct RunLocal {
    slots: Arc<Mutex<Slots>>,
}

impl RunLocal {
    /// The slot of type `T` of this run, made with its default the first
    /// time it is asked.
    pub(crate) fn slot<T: Any + Default + Send + Sync>(&self) -> Arc<T> {
        let mut slots = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
        let slot = slots
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Arc::new(T::default()))
            .clone();
        slot.downcast::<T>()
            .unwrap_or_else(|_| unreachable!("a slot holds the type it is keyed by"))
    }
}
