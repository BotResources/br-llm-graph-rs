use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use br_graph_contract::{
    CancelView, CreateError, CreateFuture, Dropped, Event, EventSender, InputReader, Instance,
    InstanceId, Node,
};
use tokio::sync::Notify;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use uuid::Uuid;

use crate::kv::{Change, Kv};

#[derive(Clone, Default)]
pub struct Gate(Arc<Notify>);

impl Gate {
    pub fn release(&self) {
        self.0.notify_one();
    }
}

#[derive(Clone, Default)]
pub struct DropFlag(Arc<AtomicBool>);

impl DropFlag {
    pub fn is_set(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

struct DropGuard(DropFlag);

impl Drop for DropGuard {
    fn drop(&mut self) {
        self.0.0.store(true, Ordering::SeqCst);
    }
}

pub enum Creation {
    Gated(Gate),
    Fail(&'static str),
    PanicText(&'static str),
    PanicString(String),
    PanicOther,
    WrongId,
    Never(DropFlag),
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct TestError(pub String);

pub struct Handle {
    pub node: String,
    pub id: InstanceId,
    pub token: CancelView,
    reader: InputReader<Kv>,
    sender: Option<EventSender<Kv>>,
    dropped: Dropped,
}

impl Handle {
    pub async fn next_input(&mut self) -> Kv {
        self.reader.next().await.unwrap().fields().clone()
    }

    pub fn accepted_inputs(&mut self) -> Vec<Kv> {
        self.reader
            .drain()
            .iter()
            .map(|input| input.fields().clone())
            .collect()
    }

    pub fn close_input(&self) {
        self.reader.close();
    }

    fn finish(&mut self, event: Event<Kv>) {
        self.sender.take().unwrap().finish(event);
    }

    pub fn result(&mut self, update: Change) {
        self.finish(Event::Result(update));
    }

    pub fn failed(&mut self, message: &str) {
        self.finish(Event::failed(TestError(message.to_owned())));
    }

    pub fn refused(&mut self, reason: &str) {
        self.finish(Event::refused(reason));
    }

    pub fn cancelled(&mut self) {
        self.finish(Event::Cancelled);
    }

    pub fn vanish(&mut self) {
        self.sender.take();
    }

    pub fn panic_in_task(&mut self) {
        let sender = self.sender.take();
        tokio::spawn(async move {
            let _sender = sender;
            panic!("the work of the node panics");
        });
    }

    pub async fn is_dropped(&mut self) -> bool {
        tokio::select! {
            biased;
            () = &mut self.dropped => true,
            () = std::future::ready(()) => false,
        }
    }
}

#[derive(Default)]
struct Inbox {
    receiver: Option<UnboundedReceiver<Handle>>,
    stash: HashMap<String, VecDeque<Handle>>,
}

pub struct Stage {
    creations: Mutex<HashMap<String, VecDeque<Creation>>>,
    calls: Mutex<Vec<(String, InstanceId)>>,
    announce: UnboundedSender<Handle>,
    inbox: tokio::sync::Mutex<Inbox>,
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

    pub async fn instance(&self, node: &str) -> Handle {
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

    fn next_creation(&self, node: &str) -> Option<Creation> {
        self.creations
            .lock()
            .unwrap()
            .get_mut(node)
            .and_then(VecDeque::pop_front)
    }
}

pub struct TestNode {
    pub name: String,
    pub kind: String,
    pub subkind: String,
    pub stage: Arc<Stage>,
}

fn other_id(id: InstanceId) -> InstanceId {
    let bytes = id.as_uuid().as_u128() ^ 1;
    InstanceId::new(Uuid::from_u128(bytes)).unwrap()
}

impl Node<Kv> for TestNode {
    fn name(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> &str {
        &self.kind
    }

    fn subkind(&self) -> &str {
        &self.subkind
    }

    fn create(&self, cancel: CancelView, id: InstanceId) -> CreateFuture<'_, Kv> {
        let creation = self.stage.next_creation(&self.name);
        self.stage
            .calls
            .lock()
            .unwrap()
            .push((self.name.clone(), id));
        let name = self.name.clone();
        let announce = self.stage.announce.clone();
        Box::pin(async move {
            let mut channel_id = id;
            match creation {
                Some(Creation::Gated(gate)) => gate.0.notified().await,
                Some(Creation::Fail(message)) => {
                    return Err(CreateError::new(TestError(message.to_owned())));
                }
                Some(Creation::PanicText(message)) => panic!("{message}"),
                Some(Creation::PanicString(message)) => panic!("{}", message),
                Some(Creation::PanicOther) => std::panic::panic_any(42_u32),
                Some(Creation::WrongId) => channel_id = other_id(id),
                Some(Creation::Never(flag)) => {
                    let _guard = DropGuard(flag);
                    std::future::pending::<()>().await;
                }
                None => {}
            }
            let (instance, reader, sender) = Instance::<Kv>::channel(channel_id);
            let dropped = sender.dropped();
            let handle = Handle {
                node: name,
                id: channel_id,
                token: cancel,
                reader,
                sender: Some(sender),
                dropped,
            };
            let _ = announce.send(handle);
            Ok(instance)
        })
    }
}
