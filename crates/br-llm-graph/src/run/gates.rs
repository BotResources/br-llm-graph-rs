//! Test gates: a body waits on the gate of its item until the test opens it,
//! so tests order completions without sleeping.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use futures_channel::oneshot;

use crate::graph::{Context, Node, NodeFuture};
use crate::state::{Config, State, Value};
use crate::update::Update;
use crate::value::Key;

#[derive(Default)]
pub(crate) struct Gates {
    waiting: Mutex<BTreeMap<String, oneshot::Receiver<()>>>,
    openers: Mutex<BTreeMap<String, oneshot::Sender<()>>>,
    log: Mutex<Vec<String>>,
}

impl Gates {
    pub(crate) fn new(items: &[&str]) -> Arc<Self> {
        let gates = Gates::default();
        for item in items {
            let (sender, receiver) = oneshot::channel();
            gates
                .waiting
                .lock()
                .unwrap()
                .insert((*item).to_owned(), receiver);
            gates
                .openers
                .lock()
                .unwrap()
                .insert((*item).to_owned(), sender);
        }
        Arc::new(gates)
    }

    pub(crate) fn open(&self, item: &str) {
        if let Some(sender) = self.openers.lock().unwrap().remove(item) {
            let _ = sender.send(());
        }
    }

    fn push(&self, line: String) {
        self.log.lock().unwrap().push(line);
    }

    pub(crate) fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    fn with_prefix(&self, prefix: &str) -> Vec<String> {
        self.log()
            .into_iter()
            .filter_map(|line| line.strip_prefix(prefix).map(str::to_owned))
            .collect()
    }

    pub(crate) fn started(&self) -> Vec<String> {
        self.with_prefix("start ")
    }

    pub(crate) fn finished(&self) -> Vec<String> {
        self.with_prefix("end ")
    }

    /// Yields until `condition` holds; panics if it never does.
    pub(crate) async fn until(&self, condition: impl Fn(&Gates) -> bool) {
        for _ in 0..10_000 {
            if condition(self) {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("the condition never held; log: {:?}", self.log());
    }

    /// Logs the start of `item`, waits until its gate is open (if it has
    /// one), then logs its end.
    pub(crate) async fn pass(&self, item: &str) {
        self.push(format!("start {item}"));
        let gate = self.waiting.lock().unwrap().remove(item);
        if let Some(gate) = gate {
            let _ = gate.await;
        }
        self.push(format!("end {item}"));
    }

    /// Opens `item` once it has started and waits until it has finished.
    pub(crate) async fn release(&self, item: &str) {
        self.until(|gates| gates.started().iter().any(|s| s == item))
            .await;
        self.open(item);
        self.until(|gates| gates.finished().iter().any(|s| s == item))
            .await;
    }
}

/// Waits on the gate of its item (the `item` key), then appends `<item>` to
/// the first list and `<item>@<list>` to every other list.
pub(crate) struct GatedBody {
    pub gates: Arc<Gates>,
    pub item: Key,
    pub lists: Vec<Key>,
}

impl Node for GatedBody {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let item = state.str(&self.item)?.to_owned();
            self.gates.pass(&item).await;
            let mut updates = Vec::new();
            for (position, list) in self.lists.iter().enumerate() {
                let value = if position == 0 {
                    item.clone()
                } else {
                    format!("{item}@{list}")
                };
                updates.push(Update::Append {
                    key: list.clone(),
                    value: Value::str(value),
                });
            }
            Ok(updates)
        })
    }
}

/// Waits on the gate of its item, then panics on `panicking`, fails on the
/// items in `failing` (`item <item> failed`), or appends the item to `outs`
/// and `ok` to `log`.
pub(crate) struct PickyBody {
    pub gates: Arc<Gates>,
    pub failing: Arc<Mutex<Vec<String>>>,
    pub panicking: Arc<Mutex<Option<String>>>,
}

impl PickyBody {
    pub(crate) fn new(gates: &Arc<Gates>, failing: &[&str]) -> Self {
        Self {
            gates: gates.clone(),
            failing: Arc::new(Mutex::new(
                failing.iter().map(|item| (*item).to_owned()).collect(),
            )),
            panicking: Arc::default(),
        }
    }
}

impl Node for PickyBody {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let item = state.str(&Key::new("item")?)?.to_owned();
            self.gates.pass(&item).await;
            if self.panicking.lock().unwrap().as_deref() == Some(item.as_str()) {
                panic!("item {item} panicked");
            }
            if self.failing.lock().unwrap().contains(&item) {
                return Err(format!("item {item} failed").into());
            }
            Ok(vec![
                Update::Append {
                    key: Key::new("outs")?,
                    value: Value::str(item),
                },
                Update::Append {
                    key: Key::new("log")?,
                    value: Value::str("ok"),
                },
            ])
        })
    }
}
