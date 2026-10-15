use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use br_llm_clients_core::{
    CacheRequirement, ClientSettings, MaxOutputTokens, Parameter, ParameterKind, Parameters,
    ReasoningLevel, ToolSpec,
};
use br_llm_clients_mock::{Capabilities, CapabilitiesSpec, MockClient, Reply};
use br_llm_graph::Tool;
use br_llm_graph::{
    Config, Context, Cursor, EndLabel, IdSource, Key, NoopObserver, Observer, Origin, Sender,
    State, ToolFuture, ToolOutput, Update, Value,
};
use br_llm_messages::{
    AssistantBlock, ModelId, Step, StopReason, StreamEvent, Text, ToolCall, ToolCallId, ToolName,
    ToolResultBlock, TurnId, UserBlock, UserInput, UserSource,
};
use serde_json::Value as Json;

pub struct PrintObserver;

impl Observer for PrintObserver {
    fn node_started(&self, _origin: &Origin, node: &br_llm_graph::NodeId) {
        println!("  node started: {node}");
    }
    fn applied(&self, _origin: &Origin, update: &Update) {
        println!("  applied: {update}");
    }
    fn checkpoint(&self, _origin: &Origin, _state: &State, cursor: &Cursor) {
        let active: Vec<&str> = cursor.active.iter().map(|n| n.as_str()).collect();
        println!("  checkpoint, next active: [{}]", active.join(", "));
    }
    fn run_finished(&self, _origin: &Origin, end: &EndLabel) {
        println!("  run finished: {end}");
    }
    fn stream(&self, origin: &Origin, key: &Key, event: &StreamEvent) {
        if let StreamEvent::TextDelta { text, .. } = event {
            println!("  stream delta ({origin}, {key}): {text:?}");
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

/// The settings of a client: a host builds them once, with the level of
/// reasoning it wants from the model.
pub fn settings(reasoning: ReasoningLevel) -> ClientSettings {
    ClientSettings::new(
        ModelId::new("example-model").expect("model id"),
        reasoning,
        CacheRequirement::NotRequired,
        MaxOutputTokens::new(4096).expect("tokens"),
    )
}

/// A client answering `steps` in order, at `reasoning`. A host gives a
/// provider library's client; the examples run offline on the scripted mock
/// of br-llm-clients-mock, which checks every request as a real client does.
/// The tool call ids of each step are renamed when it is served, so a
/// script can call tools again in one conversation.
pub fn scripted_at(reasoning: ReasoningLevel, steps: Vec<Step>) -> MockClient {
    let mut builder = MockClient::builder(
        settings(reasoning),
        Capabilities::new(CapabilitiesSpec::everything()).expect("capabilities"),
    );
    for step in steps {
        builder = builder.reply(
            Reply::success(step)
                .rename_tool_call_ids("call")
                .build()
                .expect("reply"),
        );
    }
    builder.build()
}

/// A client answering `steps` in order, reasoning at a medium level.
pub fn scripted(steps: Vec<Step>) -> Arc<MockClient> {
    Arc::new(scripted_at(ReasoningLevel::Medium, steps))
}

/// The declaration of a tool: built once, when the tool is made.
pub fn tool_spec(name: &str, description: &str, parameters: Vec<Parameter>) -> ToolSpec {
    ToolSpec::new(
        ToolName::new(name).expect("name"),
        description,
        Parameters::new(parameters).expect("parameters"),
    )
    .expect("spec")
}

/// A required string parameter.
pub fn text_parameter(name: &str, description: &str) -> Parameter {
    Parameter::new(name, description, true, ParameterKind::String).expect("parameter")
}

pub struct EchoTool {
    spec: ToolSpec,
}

impl EchoTool {
    pub fn new() -> Self {
        Self {
            spec: tool_spec("echo", "echo the message back", Vec::new()),
        }
    }
}

impl Tool for EchoTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn call<'a>(&'a self, _arguments: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("echoed").expect("non-empty"),
            )]))
        })
    }
}

pub struct SearchTool {
    spec: ToolSpec,
}

impl SearchTool {
    pub fn new() -> Self {
        Self {
            spec: tool_spec(
                "search",
                "search a knowledge base",
                vec![text_parameter("q", "what to search for")],
            ),
        }
    }
}

impl Tool for SearchTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn call<'a>(&'a self, _arguments: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("one result found").expect("non-empty"),
            )]))
        })
    }
}

/// Writes a file: counts its calls, so an example shows which ran.
pub struct WriteFileTool {
    spec: ToolSpec,
    pub calls: AtomicUsize,
}

impl WriteFileTool {
    pub fn new() -> Self {
        Self {
            spec: tool_spec(
                "write_file",
                "write a file",
                vec![
                    text_parameter("path", "where to write"),
                    text_parameter("content", "what to write"),
                ],
            ),
            calls: AtomicUsize::new(0),
        }
    }
}

impl Tool for WriteFileTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
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
    spec: ToolSpec,
    pub sender: Arc<Mutex<Option<Sender>>>,
    pub chat: Key,
    pub pending: Key,
}

impl StartTaskTool {
    pub fn new(sender: Arc<Mutex<Option<Sender>>>, chat: Key, pending: Key) -> Self {
        Self {
            spec: tool_spec(
                "start_task",
                "start background work, return at once",
                Vec::new(),
            ),
            sender,
            chat,
            pending,
        }
    }
}

impl Tool for StartTaskTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
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
