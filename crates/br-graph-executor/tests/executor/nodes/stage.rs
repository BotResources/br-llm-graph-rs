use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use br_graph_contract::InstanceId;
use br_graph_executor::Observation;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::host::within;
use crate::observer::Recorder;

use super::handle::Handle;
use super::test_node::Creation;

#[derive(Default)]
struct Inbox {
    receiver: Option<UnboundedReceiver<Handle>>,
    stash: HashMap<String, VecDeque<Handle>>,
}

pub struct Stage {
    creations: Mutex<HashMap<String, VecDeque<Creation>>>,
    pub(super) calls: Mutex<Vec<(String, InstanceId)>>,
    pub(super) announce: UnboundedSender<Handle>,
    inbox: tokio::sync::Mutex<Inbox>,
    observer: Mutex<Option<Arc<Recorder>>>,
}

impl Stage {
    pub fn new() -> Arc<Self> {
        let (announce, receiver) = unbounded_channel();
        Arc::new(Self {
            creations: Mutex::new(HashMap::new()),
            calls: Mutex::new(Vec::new()),
            announce,
            inbox: tokio::sync::Mutex::new(Inbox {
                receiver: Some(receiver),
                stash: HashMap::new(),
            }),
            observer: Mutex::new(None),
        })
    }

    pub fn script(&self, node: &str, creation: Creation) {
        self.creations
            .lock()
            .unwrap()
            .entry(node.to_owned())
            .or_default()
            .push_back(creation);
    }

    pub fn create_calls(&self, node: &str) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _)| name == node)
            .count()
    }

    pub fn call_order(&self) -> Vec<String> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    pub fn ids_of(&self, node: &str) -> Vec<InstanceId> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _)| name == node)
            .map(|(_, id)| *id)
            .collect()
    }

    pub fn attach(&self, observer: Arc<Recorder>) {
        *self.observer.lock().unwrap() = Some(observer);
    }

    pub fn detach(&self) {
        *self.observer.lock().unwrap() = None;
    }

    pub async fn instance(&self, node: &str) -> Handle {
        let handle = within(self.next_announced(node)).await;
        let observer = self.observer.lock().unwrap().clone();
        if let Some(observer) = observer {
            let id = handle.id;
            observer
                .wait_for(|observation| {
                    matches!(
                        observation,
                        Observation::InputHandedOver { instance, .. } if *instance == id
                    )
                })
                .await;
        }
        handle
    }

    async fn next_announced(&self, node: &str) -> Handle {
        let mut inbox = self.inbox.lock().await;
        loop {
            if let Some(handle) = inbox.stash.get_mut(node).and_then(VecDeque::pop_front) {
                return handle;
            }
            let handle = inbox.receiver.as_mut().unwrap().recv().await.unwrap();
            inbox
                .stash
                .entry(handle.node.clone())
                .or_default()
                .push_back(handle);
        }
    }

    pub(super) fn next_creation(&self, node: &str) -> Option<Creation> {
        self.creations
            .lock()
            .unwrap()
            .get_mut(node)
            .and_then(VecDeque::pop_front)
    }
}
