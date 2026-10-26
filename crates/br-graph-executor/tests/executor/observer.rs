use std::sync::{Arc, Mutex};

use br_graph_executor::{Observation, Observer};
use tokio::sync::Notify;

use crate::host::within;

#[derive(Default)]
pub struct Recorder {
    log: Mutex<Vec<Observation>>,
    changed: Notify,
}

impl Recorder {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn log(&self) -> Vec<Observation> {
        self.log.lock().unwrap().clone()
    }

    pub async fn wait_for(&self, matches: impl Fn(&Observation) -> bool) -> usize {
        within(self.found(matches)).await
    }

    async fn found(&self, matches: impl Fn(&Observation) -> bool) -> usize {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(position) = self.log.lock().unwrap().iter().position(&matches) {
                return position;
            }
            notified.await;
        }
    }

    pub async fn wait_for_count(&self, matches: impl Fn(&Observation) -> bool, count: usize) {
        within(self.counted(matches, count)).await;
    }

    async fn counted(&self, matches: impl Fn(&Observation) -> bool, count: usize) {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self
                .log
                .lock()
                .unwrap()
                .iter()
                .filter(|o| matches(o))
                .count()
                >= count
            {
                return;
            }
            notified.await;
        }
    }

    pub async fn wait_for_outcome(&self) -> Vec<Observation> {
        self.wait_for(|o| matches!(o, Observation::Outcome(_)))
            .await;
        self.log()
    }
}

impl Observer for Recorder {
    fn observe(&self, observation: Observation) {
        self.log.lock().unwrap().push(observation);
        self.changed.notify_waiters();
    }
}

pub fn created(node: &'static str) -> impl Fn(&Observation) -> bool {
    move |o| matches!(o, Observation::InstanceCreated { node: n, .. } if n == node)
}

pub fn handed(node: &'static str) -> impl Fn(&Observation) -> bool {
    move |o| matches!(o, Observation::InputHandedOver { node: n, .. } if n == node)
}

pub fn handled(node: &'static str) -> impl Fn(&Observation) -> bool {
    move |o| matches!(o, Observation::EventHandled { node: n, .. } if n == node)
}

pub fn edge_evaluated(node: &'static str) -> impl Fn(&Observation) -> bool {
    move |o| matches!(o, Observation::EdgeEvaluated { node: n, .. } if n == node)
}

pub fn update_applied(node: &'static str) -> impl Fn(&Observation) -> bool {
    move |o| matches!(o, Observation::UpdateApplied { node: n } if n == node)
}

pub fn position(log: &[Observation], matches: impl Fn(&Observation) -> bool) -> usize {
    log.iter().position(matches).unwrap()
}

pub fn count(log: &[Observation], matches: impl Fn(&Observation) -> bool) -> usize {
    log.iter().filter(|o| matches(o)).count()
}
