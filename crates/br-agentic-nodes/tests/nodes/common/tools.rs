use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_agentic_contract::{ResultStream, Tool, ToolCode, ToolContent, ToolFault};
use br_graph_contract::CancelView;
use br_llm_clients_core::{BoxFuture, Parameters, ToolSpec};
use br_llm_messages::{OpaqueJson, ToolName};

use super::ids::block_id;
use super::text_block;

pub enum Behavior {
    Text(String),
    ErrorText(String),
    Slow(Duration, String),
    Hang,
    Fault,
    Panic,
}

pub struct Probe {
    behavior: Behavior,
    pub starts: Mutex<usize>,
    pub arguments: Mutex<Vec<String>>,
    pub cancel_views: Mutex<Vec<bool>>,
}

impl ToolCode for Probe {
    fn call<'a>(
        &'a self,
        arguments: &'a OpaqueJson,
        cancel: CancelView,
        result: &'a mut dyn ResultStream,
    ) -> BoxFuture<'a, Result<ToolContent, ToolFault>> {
        Box::pin(async move {
            *self.starts.lock().unwrap() += 1;
            self.arguments
                .lock()
                .unwrap()
                .push(arguments.to_json_string().unwrap());
            self.cancel_views
                .lock()
                .unwrap()
                .push(cancel.is_triggered());
            match &self.behavior {
                Behavior::Text(text) => {
                    result.send(text.clone()).await;
                    Ok(ToolContent::new(vec![text_block(text)], false))
                }
                Behavior::ErrorText(text) => {
                    Ok(ToolContent::error_text(block_id(), text.clone()).unwrap())
                }
                Behavior::Slow(delay, text) => {
                    tokio::time::sleep(*delay).await;
                    Ok(ToolContent::new(vec![text_block(text)], false))
                }
                Behavior::Hang => std::future::pending().await,
                Behavior::Fault => Err(ToolFault::new("the tool broke")),
                Behavior::Panic => panic!("the tool panicked"),
            }
        })
    }
}

pub fn spec(name: &str) -> ToolSpec {
    ToolSpec::new(
        ToolName::new(name).unwrap(),
        "a test tool",
        Parameters::default(),
    )
    .unwrap()
}

pub fn tool(name: &str, behavior: Behavior) -> (Tool, Arc<Probe>) {
    let probe = Arc::new(Probe {
        behavior,
        starts: Mutex::new(0),
        arguments: Mutex::new(Vec::new()),
        cancel_views: Mutex::new(Vec::new()),
    });
    (Tool::new(spec(name), probe.clone()), probe)
}

pub fn echo(name: &str) -> (Tool, Arc<Probe>) {
    tool(name, Behavior::Text(format!("{name} done")))
}
