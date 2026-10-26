mod child;
mod modes;
mod order;
mod watch;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use br_graph_contract::{
    CancelHandle, CancelView, CreateError, CreateFuture, Event, Instance, InstanceId, Node,
    PROGRAMMATIC,
};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use uuid::Uuid;

pub use self::modes::{CreateMode, Mode, ProbeCause};
pub use self::order::{Log, Mark, Witness};
pub use self::watch::Watch;
use self::watch::{Gates, Live, Stats, locked};
use super::{CREATION_GATE, Item, ItemUpdate};

struct Shared {
    create_mode: Box<dyn Fn(usize) -> CreateMode + Send + Sync>,
    mode: Box<dyn Fn(u32) -> Mode + Send + Sync>,
    stats: Arc<Stats>,
    started: UnboundedSender<u32>,
    dropped: UnboundedSender<usize>,
    gates: Arc<Gates>,
    log: Arc<Log>,
    outer: Arc<Mutex<Option<CancelHandle>>>,
}

pub struct Probe {
    shared: Arc<Shared>,
}

impl Probe {
    pub fn new(
        create_mode: impl Fn(usize) -> CreateMode + Send + Sync + 'static,
        mode: impl Fn(u32) -> Mode + Send + Sync + 'static,
    ) -> (Arc<Probe>, Watch) {
        let (started_tx, started) = unbounded_channel();
        let (dropped_tx, dropped) = unbounded_channel();
        let stats = Arc::new(Stats::default());
        let gates = Arc::new(Gates::default());
        let log = Arc::new(Log::default());
        let outer = Arc::new(Mutex::new(None));
        let shared = Arc::new(Shared {
            create_mode: Box::new(create_mode),
            mode: Box::new(mode),
            stats: Arc::clone(&stats),
            started: started_tx,
            dropped: dropped_tx,
            gates: Arc::clone(&gates),
            log: Arc::clone(&log),
            outer: Arc::clone(&outer),
        });
        let watch = Watch {
            started,
            dropped,
            gates,
            log,
            stats,
            outer,
        };
        (Arc::new(Probe { shared }), watch)
    }

    pub fn with_mode(mode: impl Fn(u32) -> Mode + Send + Sync + 'static) -> (Arc<Probe>, Watch) {
        Self::new(|_| CreateMode::Normal, mode)
    }
}

impl Node<Item> for Probe {
    fn name(&self) -> &str {
        "NOT a valid node name"
    }

    fn kind(&self) -> &str {
        PROGRAMMATIC
    }

    fn subkind(&self) -> &str {
        "probe"
    }

    fn create(&self, cancel: CancelView, id: InstanceId) -> CreateFuture<'_, Item> {
        let shared = Arc::clone(&self.shared);
        shared.stats.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            let position = shared.stats.created.fetch_add(1, Ordering::SeqCst);
            let create_mode = (shared.create_mode)(position);
            match create_mode {
                CreateMode::FailCreate => return Err(CreateError::new(ProbeCause)),
                CreateMode::Hang => std::future::pending::<()>().await,
                CreateMode::FailWhenReleased => {
                    shared.gates.gate(CREATION_GATE).notified().await;
                    return Err(CreateError::new(ProbeCause));
                }
                CreateMode::GateThenNormal => {
                    shared.gates.gate(CREATION_GATE).notified().await;
                }
                _ => {}
            }
            locked(&shared.stats.ids).push(id);
            locked(&shared.stats.views).push(cancel.clone());
            let given = match create_mode {
                CreateMode::WrongId => InstanceId::new(Uuid::now_v7()).expect("a v7 id"),
                _ => id,
            };
            let (instance, reader, sender) = Instance::channel(given);
            if matches!(create_mode, CreateMode::CloseInput) {
                reader.close();
            }
            if matches!(create_mode, CreateMode::AnswerAndClose) {
                reader.close();
                sender.finish(Event::Result(ItemUpdate::Set(0)));
                return Ok(instance);
            }
            let live = Live::enter(&shared.stats);
            tokio::spawn(child::child(shared, live, position, reader, sender, cancel));
            Ok(instance)
        })
    }
}
