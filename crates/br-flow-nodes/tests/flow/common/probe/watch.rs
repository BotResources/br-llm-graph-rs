use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use br_graph_contract::{CancelHandle, CancelView, InstanceId};
use tokio::sync::Notify;
use tokio::sync::mpsc::UnboundedReceiver;

#[derive(Default)]
pub struct Gates {
    map: Mutex<HashMap<u32, Arc<Notify>>>,
}

pub(super) fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Gates {
    pub(super) fn gate(&self, value: u32) -> Arc<Notify> {
        Arc::clone(locked(&self.map).entry(value).or_default())
    }

    pub fn release(&self, value: u32) {
        self.gate(value).notify_one();
    }
}

#[derive(Default)]
pub(super) struct Stats {
    pub(super) created: AtomicUsize,
    pub(super) live: AtomicUsize,
    pub(super) max_live: AtomicUsize,
    pub(super) extra_inputs: AtomicUsize,
    pub(super) ids: Mutex<Vec<InstanceId>>,
    pub(super) views: Mutex<Vec<CancelView>>,
    pub(super) inputs: Mutex<Vec<(InstanceId, u32)>>,
}

pub struct Watch {
    pub started: UnboundedReceiver<u32>,
    pub dropped: UnboundedReceiver<usize>,
    pub gates: Arc<Gates>,
    pub(super) stats: Arc<Stats>,
    pub(super) outer: Arc<Mutex<Option<CancelHandle>>>,
}

impl Watch {
    pub fn created(&self) -> usize {
        self.stats.created.load(Ordering::SeqCst)
    }

    pub fn max_live(&self) -> usize {
        self.stats.max_live.load(Ordering::SeqCst)
    }

    pub fn extra_inputs(&self) -> usize {
        self.stats.extra_inputs.load(Ordering::SeqCst)
    }

    pub fn ids(&self) -> Vec<InstanceId> {
        locked(&self.stats.ids).clone()
    }

    pub fn views(&self) -> Vec<CancelView> {
        locked(&self.stats.views).clone()
    }

    pub fn values_in_creation_order(&self) -> Vec<u32> {
        let inputs = locked(&self.stats.inputs).clone();
        self.ids()
            .iter()
            .filter_map(|id| {
                inputs
                    .iter()
                    .find(|(seen, _)| seen == id)
                    .map(|(_, value)| *value)
            })
            .collect()
    }

    pub fn outer_trigger(&self, handle: CancelHandle) {
        *locked(&self.outer) = Some(handle);
    }

    pub async fn started_values(&mut self, count: usize) -> Vec<u32> {
        let mut values = Vec::new();
        for _ in 0..count {
            values.push(self.started.recv().await.expect("a started child"));
        }
        values
    }

    pub async fn dropped_positions(&mut self, count: usize) -> Vec<usize> {
        let mut values = Vec::new();
        for _ in 0..count {
            values.push(self.dropped.recv().await.expect("a dropped child"));
        }
        values.sort_unstable();
        values
    }
}

pub(super) struct Live(Arc<Stats>);

impl Live {
    pub(super) fn enter(stats: &Arc<Stats>) -> Self {
        let now = stats.live.fetch_add(1, Ordering::SeqCst) + 1;
        stats.max_live.fetch_max(now, Ordering::SeqCst);
        Self(Arc::clone(stats))
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.0.live.fetch_sub(1, Ordering::SeqCst);
    }
}
