use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use br_graph_executor::{Checkpoint, CheckpointSink, SinkFuture, SinkRefusal};
use tokio::sync::Notify;

use crate::host::within;

pub enum SinkBehavior {
    Refuse(&'static str),
    Panic,
    Block(Arc<Notify>),
    BlockThenRefuse(Arc<Notify>, &'static str),
}

#[derive(Default)]
pub struct RecordingSink {
    script: Mutex<VecDeque<SinkBehavior>>,
    received: Mutex<Vec<Checkpoint>>,
    entered: Notify,
    active: AtomicUsize,
    overlaps: AtomicUsize,
}

struct InFlight<'a>(&'a AtomicUsize);

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl RecordingSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn script(&self, behavior: SinkBehavior) {
        self.script.lock().unwrap().push_back(behavior);
    }

    pub fn received(&self) -> Vec<Checkpoint> {
        self.received.lock().unwrap().clone()
    }

    pub fn calls(&self) -> usize {
        self.received.lock().unwrap().len()
    }

    pub fn in_flight(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }

    pub fn overlaps(&self) -> usize {
        self.overlaps.load(Ordering::SeqCst)
    }

    pub async fn wait_for_calls(&self, calls: usize) {
        within(self.reached(calls)).await;
    }

    async fn reached(&self, calls: usize) {
        loop {
            let notified = self.entered.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.calls() >= calls {
                return;
            }
            notified.await;
        }
    }
}

impl CheckpointSink for RecordingSink {
    fn accept<'a>(&'a self, checkpoint: &'a Checkpoint) -> SinkFuture<'a> {
        Box::pin(async move {
            if self.active.fetch_add(1, Ordering::SeqCst) > 0 {
                self.overlaps.fetch_add(1, Ordering::SeqCst);
            }
            let _in_flight = InFlight(&self.active);
            self.received.lock().unwrap().push(checkpoint.clone());
            self.entered.notify_waiters();
            let behavior = self.script.lock().unwrap().pop_front();
            let answer = match behavior {
                None => Ok(()),
                Some(SinkBehavior::Refuse(reason)) => Err(SinkRefusal::new(reason)),
                Some(SinkBehavior::Panic) => panic!("the sink panics"),
                Some(SinkBehavior::Block(gate)) => {
                    gate.notified().await;
                    Ok(())
                }
                Some(SinkBehavior::BlockThenRefuse(gate, reason)) => {
                    gate.notified().await;
                    Err(SinkRefusal::new(reason))
                }
            };
            answer
        })
    }
}
