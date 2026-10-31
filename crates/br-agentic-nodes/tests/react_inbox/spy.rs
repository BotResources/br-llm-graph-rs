use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_agentic_contract::memory::MemoryInbox;
use br_agentic_contract::{
    Cursor, InboxBatch, InboxContext, InboxPort, MailDocument, MailId, NewMail, PortError,
    PortFuture,
};
use br_llm_messages::EntryId;

use crate::common::Rig;

pub enum Items {
    Pass,
    Fail,
    Hang,
    Sleep(Duration),
    Give(InboxBatch),
}

pub enum Act {
    Pass,
    Fault,
    Refuse(String),
    Hang,
    Sleep(Duration),
}

pub enum Docs {
    Pass,
    Fault,
    Refuse(String),
    Hang,
    Give(Vec<MailDocument>),
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
    pub inner: Arc<MemoryInbox>,
    items: Mutex<VecDeque<Items>>,
    marks: Mutex<VecDeque<Act>>,
    sends: Mutex<VecDeque<Act>>,
    docs: Mutex<VecDeque<Docs>>,
    calls: AtomicUsize,
    live: AtomicUsize,
    peak: AtomicUsize,
    waits: AtomicUsize,
    asked_documents: AtomicUsize,
    dropped_reads: Arc<AtomicUsize>,
    dropped_marks: Arc<AtomicUsize>,
    cursors: Mutex<Vec<Option<String>>>,
    marked: Mutex<Vec<EntryId>>,
}

impl Spy {
    pub fn new(rig: &Rig) -> Arc<Self> {
        Arc::new(Self {
            inner: rig.inbox.clone(),
            items: Mutex::default(),
            marks: Mutex::default(),
            sends: Mutex::default(),
            docs: Mutex::default(),
            calls: AtomicUsize::new(0),
            live: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            waits: AtomicUsize::new(0),
            asked_documents: AtomicUsize::new(0),
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

    pub fn plan_sends(&self, plan: Vec<Act>) -> &Self {
        self.sends.lock().unwrap().extend(plan);
        self
    }

    pub fn plan_docs(&self, plan: Vec<Docs>) -> &Self {
        self.docs.lock().unwrap().extend(plan);
        self
    }

    pub fn reads(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    pub fn peak_reads(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }

    pub fn waits(&self) -> usize {
        self.waits.load(Ordering::SeqCst)
    }

    pub fn documents_asked(&self) -> usize {
        self.asked_documents.load(Ordering::SeqCst)
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

impl InboxPort for Spy {
    fn items<'a>(
        &'a self,
        context: &'a InboxContext,
        after: Option<&'a Cursor>,
        limit: NonZeroUsize,
    ) -> PortFuture<'a, Result<InboxBatch, PortError>> {
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

    fn wait<'a>(&'a self, context: &'a InboxContext) -> PortFuture<'a, Result<(), PortError>> {
        self.waits.fetch_add(1, Ordering::SeqCst);
        self.inner.wait(context)
    }

    fn mark_read<'a>(
        &'a self,
        context: &'a InboxContext,
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

    fn send<'a>(
        &'a self,
        context: &'a InboxContext,
        mail: NewMail,
    ) -> PortFuture<'a, Result<(), PortError>> {
        let plan = self.sends.lock().unwrap().pop_front().unwrap_or(Act::Pass);
        Box::pin(async move {
            match plan {
                Act::Pass => self.inner.send(context, mail).await,
                Act::Fault => Err(PortError::fault("the send failed")),
                Act::Refuse(reason) => Err(PortError::refused(reason)),
                Act::Hang => std::future::pending().await,
                Act::Sleep(delay) => {
                    tokio::time::sleep(delay).await;
                    self.inner.send(context, mail).await
                }
            }
        })
    }

    fn documents<'a>(
        &'a self,
        context: &'a InboxContext,
        mail: MailId,
    ) -> PortFuture<'a, Result<Vec<MailDocument>, PortError>> {
        self.asked_documents.fetch_add(1, Ordering::SeqCst);
        let plan = self.docs.lock().unwrap().pop_front().unwrap_or(Docs::Pass);
        Box::pin(async move {
            match plan {
                Docs::Pass => self.inner.documents(context, mail).await,
                Docs::Fault => Err(PortError::fault("the documents are gone")),
                Docs::Refuse(reason) => Err(PortError::refused(reason)),
                Docs::Hang => std::future::pending().await,
                Docs::Give(documents) => Ok(documents),
            }
        })
    }
}
