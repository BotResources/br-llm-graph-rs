//! A test body that counts its calls per item and can be told to fail on one
//! item, to drive failures and resumes.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::graph::{Context, Node, NodeFuture};
use crate::state::{Config, State, Value};
use crate::update::Update;
use crate::value::Key;

#[derive(Default)]
pub(crate) struct Counted {
    calls: Mutex<BTreeMap<String, usize>>,
    fail_on: Mutex<Option<String>>,
}

impl Counted {
    pub(crate) fn failing_on(item: &str) -> Arc<Self> {
        let counted = Counted::default();
        *counted.fail_on.lock().unwrap() = Some(item.to_owned());
        Arc::new(counted)
    }

    pub(crate) fn heal(&self) {
        *self.fail_on.lock().unwrap() = None;
    }

    pub(crate) fn calls(&self) -> BTreeMap<String, usize> {
        self.calls.lock().unwrap().clone()
    }
}

/// Appends the upper-cased item to `outs` and `<item>@log` to `log`.
pub(crate) struct CountedBody {
    pub counted: Arc<Counted>,
    pub item: Key,
    pub outs: Key,
    pub log: Key,
}

impl Node for CountedBody {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let item = state.str(&self.item)?.to_owned();
            *self
                .counted
                .calls
                .lock()
                .unwrap()
                .entry(item.clone())
                .or_default() += 1;
            if self.counted.fail_on.lock().unwrap().as_deref() == Some(item.as_str()) {
                return Err(format!("item {item} failed").into());
            }
            Ok(vec![
                Update::Append {
                    key: self.outs.clone(),
                    value: Value::str(item.to_uppercase()),
                },
                Update::Append {
                    key: self.log.clone(),
                    value: Value::str(format!("{item}@log")),
                },
            ])
        })
    }
}
