use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_agentic_contract::memory::MemoryIncoming;
use br_agentic_contract::{
    Cursor, IncomingBatch, IncomingContext, IncomingPort, PortError, PortFuture,
};
use br_llm_messages::EntryId;

use crate::common::Rig;

pub enum Items {
    Pass,
    Fail,
    Hang,
    Sleep(Duration),
    Give(IncomingBatch),
}

pub enum Act {
    Pass,
    Fault,
    Refuse(String),
    Hang,
    Sleep(Duration),
}

struct Unfinished {
    counter: Arc<AtomicUsize>,
    done: bool,
}

impl Drop for Unfinished {
    fn drop(&mut self) {
        if !self.done {
            self.counter.fetch_add(1, Ordering::SeqCst);
        }
    }
}

struct Live<'a> {
    spy: &'a Spy,
}

impl<'a> Live<'a> {
    fn enter(spy: &'a Spy) -> Self {
        let now = spy.live.fetch_add(1, Ordering::SeqCst) + 1;
        spy.peak.fetch_max(now, Ordering::SeqCst);
        Self { spy }
    }
}

impl Drop for Live<'_> {
    fn drop(&mut self) {
        self.spy.live.fetch_sub(1, Ordering::SeqCst);
    }
}

pub struct Spy {
    pub inner: Arc<MemoryIncoming>,
    items: Mutex<VecDeque<Items>>,
    marks: Mutex<VecDeque<Act>>,
    calls: AtomicUsize,
    live: AtomicUsize,
    peak: AtomicUsize,
    dropped_reads: Arc<AtomicUsize>,
    dropped_marks: Arc<AtomicUsize>,
    cursors: Mutex<Vec<Option<String>>>,
    marked: Mutex<Vec<EntryId>>,
}

impl Spy {
    pub fn new(rig: &Rig) -> Arc<Self> {
        Arc::new(Self {
            inner: rig.incoming.clone(),
            items: Mutex::default(),
            marks: Mutex::default(),
            calls: AtomicUsize::new(0),
            live: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            dropped_reads: Arc::default(),
            dropped_marks: Arc::default(),
            cursors: Mutex::default(),
            marked: Mutex::default(),
        })
    }

    pub fn plan_items(&self, plan: Vec<Items>) -> &Self {
        self.items.lock().unwrap().extend(plan);
        self
    }

    pub fn plan_marks(&self, plan: Vec<Act>) -> &Self {
        self.marks.lock().unwrap().extend(plan);
        self
    }

    pub fn reads(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    pub fn peak_reads(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }

    pub fn dropped_reads(&self) -> usize {
        self.dropped_reads.load(Ordering::SeqCst)
    }

    pub fn dropped_marks(&self) -> usize {
        self.dropped_marks.load(Ordering::SeqCst)
    }

    pub fn cursors(&self) -> Vec<Option<String>> {
        self.cursors.lock().unwrap().clone()
    }

    pub fn marked(&self) -> Vec<EntryId> {
        self.marked.lock().unwrap().clone()
    }

    pub fn times_marked(&self, item: EntryId) -> usize {
        self.marked()
            .iter()
            .filter(|marked| **marked == item)
            .count()
    }
}

impl IncomingPort for Spy {
    fn items<'a>(
        &'a self,
        context: &'a IncomingContext,
        after: Option<&'a Cursor>,
        limit: NonZeroUsize,
    ) -> PortFuture<'a, Result<IncomingBatch, PortError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.cursors
            .lock()
            .unwrap()
            .push(after.map(|cursor| cursor.as_str().to_owned()));
        let plan = self
            .items
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Items::Pass);
        Box::pin(async move {
            let _live = Live::enter(self);
            let mut guard = Unfinished {
                counter: self.dropped_reads.clone(),
                done: false,
            };
            let outcome = match plan {
                Items::Pass => self.inner.items(context, after, limit).await,
                Items::Fail => Err(PortError::fault("the read failed")),
                Items::Hang => std::future::pending().await,
                Items::Sleep(delay) => {
                    tokio::time::sleep(delay).await;
                    self.inner.items(context, after, limit).await
                }
                Items::Give(batch) => Ok(batch),
            };
            guard.done = true;
            outcome
        })
    }

    fn mark_read<'a>(
        &'a self,
        context: &'a IncomingContext,
        item: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        let plan = self.marks.lock().unwrap().pop_front().unwrap_or(Act::Pass);
        Box::pin(async move {
            let mut guard = Unfinished {
                counter: self.dropped_marks.clone(),
                done: false,
            };
            let outcome = match plan {
                Act::Pass => self.inner.mark_read(context, item).await,
                Act::Fault => Err(PortError::fault("the mark failed")),
                Act::Refuse(reason) => Err(PortError::refused(reason)),
                Act::Hang => std::future::pending().await,
                Act::Sleep(delay) => {
                    tokio::time::sleep(delay).await;
                    self.inner.mark_read(context, item).await
                }
            };
            guard.done = true;
            if outcome.is_ok() {
                self.marked.lock().unwrap().push(item);
            }
            outcome
        })
    }
}
