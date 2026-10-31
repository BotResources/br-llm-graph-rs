use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_agentic_contract::memory::Record;
use br_agentic_contract::{
    PortFuture, ResultStream, Tool, ToolBlock, ToolCode, ToolContent, ToolFault,
};
use br_graph_contract::CancelView;
use br_llm_messages::OpaqueJson;
use tokio::sync::Barrier;

use crate::common::{names, spec};

pub enum Plan {
    Text(String),
    After(Duration, String),
    Gate(Arc<Barrier>),
    Hang,
    Fault,
    Guarded(Arc<AtomicBool>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub opened: usize,
    pub completed: usize,
}

pub fn counts(record: &Record) -> Counts {
    let names = names(record);
    Counts {
        opened: names.iter().filter(|name| **name == "open_result").count(),
        completed: names
            .iter()
            .filter(|name| **name == "complete_result")
            .count(),
    }
}

pub struct Spy {
    plan: Plan,
    record: Record,
    pub started: Mutex<Vec<Counts>>,
    pub ended: Mutex<Vec<Counts>>,
    pub arguments: Mutex<Vec<String>>,
    pub views: Mutex<Vec<CancelView>>,
    pub triggered_at_start: Mutex<Vec<bool>>,
}

struct Flag(Arc<AtomicBool>);

impl Drop for Flag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

impl Spy {
    pub fn starts(&self) -> usize {
        self.started.lock().unwrap().len()
    }
}

fn answer(text: &str) -> ToolContent {
    ToolContent::new(vec![ToolBlock::text(text).unwrap()], false)
}

impl ToolCode for Spy {
    fn call<'a>(
        &'a self,
        arguments: &'a OpaqueJson,
        cancel: CancelView,
        result: &'a mut dyn ResultStream,
    ) -> PortFuture<'a, Result<ToolContent, ToolFault>> {
        Box::pin(async move {
            self.started.lock().unwrap().push(counts(&self.record));
            self.arguments
                .lock()
                .unwrap()
                .push(arguments.to_json_string().unwrap());
            self.triggered_at_start
                .lock()
                .unwrap()
                .push(cancel.is_triggered());
            self.views.lock().unwrap().push(cancel);
            let content = match &self.plan {
                Plan::Text(text) => {
                    result.send(text.clone()).await;
                    answer(text)
                }
                Plan::After(delay, text) => {
                    tokio::time::sleep(*delay).await;
                    answer(text)
                }
                Plan::Gate(barrier) => {
                    barrier.wait().await;
                    answer("through the gate")
                }
                Plan::Hang => std::future::pending().await,
                Plan::Fault => return Err(ToolFault::new("the tool broke")),
                Plan::Guarded(dropped) => {
                    let _flag = Flag(dropped.clone());
                    std::future::pending().await
                }
            };
            self.ended.lock().unwrap().push(counts(&self.record));
            Ok(content)
        })
    }
}

pub fn spy(name: &str, plan: Plan, record: &Record) -> (Tool, Arc<Spy>) {
    let spy = Arc::new(Spy {
        plan,
        record: record.clone(),
        started: Mutex::new(Vec::new()),
        ended: Mutex::new(Vec::new()),
        arguments: Mutex::new(Vec::new()),
        views: Mutex::new(Vec::new()),
        triggered_at_start: Mutex::new(Vec::new()),
    });
    (Tool::new(spec(name), spy.clone()), spy)
}
