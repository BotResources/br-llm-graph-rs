use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_graph_contract::{
    CancelView, CreateFuture, Event, Instance, InstanceId, Node, PROGRAMMATIC,
};

use super::panic_waker::register_panicking_waker;
use super::{Item, ItemUpdate};

pub struct PoisonedFirst {
    calls: AtomicUsize,
}

impl PoisonedFirst {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
        })
    }

    pub fn create_calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Node<Item> for PoisonedFirst {
    fn name(&self) -> &str {
        "NOT a valid node name"
    }

    fn kind(&self) -> &str {
        PROGRAMMATIC
    }

    fn subkind(&self) -> &str {
        "probe"
    }

    fn create(&self, _cancel: CancelView, id: InstanceId) -> CreateFuture<'_, Item> {
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if index > 0 {
                std::future::pending::<()>().await;
            }
            let (instance, mut reader, sender) = Instance::<Item>::channel(id);
            tokio::spawn(async move {
                let Some(input) = reader.next().await else {
                    return;
                };
                let value = input.fields().value;
                let dropped = register_panicking_waker(sender.dropped());
                sender.finish(Event::Result(ItemUpdate::Set(value * 2)));
                let _held = dropped;
                std::future::pending::<()>().await;
            });
            Ok(instance)
        })
    }
}
