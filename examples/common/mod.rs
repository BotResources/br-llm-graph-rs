use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use br_llm_graph::Tool;
use br_llm_graph::{
    Config, Context, Cursor, EndLabel, IdSource, Key, Model, ModelFuture, NoopObserver, Observer,
    Request, Sender, State, StreamSink, ToolFuture, ToolOutput, ToolSpec, Update, Value,
};
use br_llm_messages::{
    AssistantBlock, Step, StopReason, StreamEvent, Text, ToolCall, ToolCallId, ToolName,
    ToolResultBlock, TurnId, UserBlock, UserInput, UserSource,
};
use serde_json::{Value as Json, json};

pub struct PrintObserver;

impl Observer for PrintObserver {
    fn node_started(&self, node: &br_llm_graph::NodeId) {
        println!("  node started: {node}");
    }
    fn applied(&self, update: &Update) {
        println!("  applied: {update}");
    }
    fn checkpoint(&self, _state: &State, cursor: &Cursor) {
        let active: Vec<&str> = cursor.active.iter().map(|n| n.as_str()).collect();
        println!("  checkpoint, next active: [{}]", active.join(", "));
    }
    fn run_finished(&self, end: &EndLabel) {
        println!("  run finished: {end}");
    }
    fn stream(&self, _key: &Key, event: &StreamEvent) {
        if let StreamEvent::TextDelta { text, .. } = event {
            println!("  stream delta: {text:?}");
        }
    }
}

pub struct SeqIds {
    next: AtomicU64,
}

impl SeqIds {
    pub fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
        }
    }
}

impl IdSource for SeqIds {
    fn turn_id(&self) -> TurnId {
        let index = self.next.fetch_add(1, Ordering::SeqCst);
        TurnId::new(format!("turn-{index}")).expect("non-empty turn id")
    }
}

pub fn context() -> Context {
    Context::new(Arc::new(PrintObserver), Arc::new(SeqIds::new()))
}

pub fn quiet_context() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

pub fn key(name: &str) -> Key {
    Key::new(name).expect("valid key")
}

pub fn user(body: &str) -> UserInput {
    UserInput::new(
        UserSource::Human,
        None,
        vec![UserBlock::text(Text::new(body).expect("non-empty"))],
    )
    .expect("valid input")
}

pub fn runtime_input(kind: &str, body: &str) -> UserInput {
    UserInput::new(
        UserSource::runtime(kind).expect("kind"),
        None,
        vec![UserBlock::text(Text::new(body).expect("non-empty"))],
    )
    .expect("valid input")
}

pub fn text_step(body: &str) -> Step {
    Step::new(
        vec![AssistantBlock::Text {
            text: Text::new(body).expect("non-empty"),
        }],
        StopReason::EndTurn,
        None,
        None,
    )
    .expect("valid step")
}

pub fn tool_call_step(id: &str, name: &str, arguments: Json) -> Step {
    Step::new(
        vec![AssistantBlock::ToolCall(ToolCall {
            id: ToolCallId::new(id).expect("id"),
            name: ToolName::new(name).expect("name"),
            arguments,
        })],
        StopReason::AwaitingToolResults,
        None,
        None,
    )
    .expect("valid step")
}

pub fn structured_step(value: Json) -> Step {
    Step::new(
        vec![AssistantBlock::Structured { value }],
        StopReason::EndTurn,
        None,
        None,
    )
    .expect("valid step")
}

pub struct ScriptedModel {
    steps: Mutex<VecDeque<Step>>,
}

impl ScriptedModel {
    pub fn new(steps: Vec<Step>) -> Self {
        Self {
            steps: Mutex::new(steps.into_iter().collect()),
        }
    }
}

impl Model for ScriptedModel {
    fn complete<'a>(&'a self, _request: Request, sink: &'a dyn StreamSink) -> ModelFuture<'a> {
        let step = self
            .steps
            .lock()
            .ok()
            .and_then(|mut queue| queue.pop_front());
        Box::pin(async move {
            let step = step
                .ok_or_else(|| -> br_llm_graph::ModelError { "scripted model exhausted".into() })?;
            for text in step.text() {
                sink.event(&StreamEvent::TextDelta {
                    index: 0,
                    text: text.as_str().to_owned(),
                });
            }
            Ok(step)
        })
    }
}

fn spec(name: &str, description: &str) -> ToolSpec {
    ToolSpec {
        name: ToolName::new(name).expect("name"),
        description: description.to_owned(),
        parameters: json!({ "type": "object" }),
    }
}

pub struct EchoTool;

impl Tool for EchoTool {
    fn spec(&self) -> ToolSpec {
        spec("echo", "echo the message back")
    }
    fn safe(&self) -> bool {
        true
    }
    fn call<'a>(&'a self, _arguments: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("echoed").expect("non-empty"),
            )]))
        })
    }
}

pub struct SearchTool;

impl Tool for SearchTool {
    fn spec(&self) -> ToolSpec {
        spec("search", "search a knowledge base")
    }
    fn safe(&self) -> bool {
        true
    }
    fn call<'a>(&'a self, _arguments: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("one result found").expect("non-empty"),
            )]))
        })
    }
}

#[derive(Default)]
pub struct WriteFileTool {
    pub calls: AtomicUsize,
}

impl Tool for WriteFileTool {
    fn spec(&self) -> ToolSpec {
        spec("write_file", "write a file (unsafe)")
    }
    fn safe(&self) -> bool {
        false
    }
    fn call<'a>(&'a self, _arguments: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("file written").expect("non-empty"),
            )]))
        })
    }
}

pub struct StartTaskTool {
    pub sender: Arc<Mutex<Option<Sender>>>,
    pub chat: Key,
    pub pending: Key,
}

impl Tool for StartTaskTool {
    fn spec(&self) -> ToolSpec {
        spec("start_task", "start background work, return at once")
    }
    fn safe(&self) -> bool {
        true
    }
    fn call<'a>(&'a self, _arguments: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        let chat = self.chat.clone();
        let handle = self.sender.lock().ok().and_then(|guard| guard.clone());
        if let Some(sender) = handle {
            tokio::spawn(async move {
                tokio::task::yield_now().await;
                sender.send(chat, runtime_input("task_done", "task-1 completed"));
            });
        }
        let pending = self.pending.clone();
        Box::pin(async move {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("task started").expect("non-empty"),
            )])
            .with_updates(vec![Update::Append {
                key: pending,
                value: Value::str("task-1"),
            }]))
        })
    }
}
