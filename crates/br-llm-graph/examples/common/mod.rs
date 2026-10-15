pub mod tools;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use br_llm_clients_core::{CacheRequirement, ClientSettings, MaxOutputTokens, ReasoningLevel};
use br_llm_clients_mock::{Capabilities, CapabilitiesSpec, MockClient, Reply};
use br_llm_graph::{
    Context, Cursor, EndLabel, IdSource, Key, ModelNode, NoopObserver, Observer, Origin, Prompt,
    Schema, State, Update,
};
use br_llm_messages::{
    AssistantBlock, InputId, ModelId, Step, StopReason, StreamEvent, Text, ToolCall, ToolCallId,
    ToolName, TurnId, UserBlock, UserInput, UserSource,
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

/// The text the string configuration key `name` holds: the system text most
/// of these examples read.
pub fn config_text<N: ModelNode>(name: &str) -> Prompt<N> {
    let mut keys = Schema::builder();
    Prompt::from_config(&keys.config::<String>(name).expect("valid key"))
}

pub fn user(body: &str) -> UserInput {
    UserInput::new(
        InputId::new(format!("i-{body}")).expect("input id"),
        UserSource::Human,
        None,
        vec![UserBlock::text(Text::new(body).expect("non-empty"))],
    )
    .expect("valid input")
}

pub fn runtime_input(kind: &str, body: &str) -> UserInput {
    UserInput::new(
        InputId::new(format!("i-{body}")).expect("input id"),
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

/// A client answering `steps` in order, reasoning at a medium level, with
/// the tool call ids as the script writes them: each must be unique in the
/// conversation.
pub fn scripted_as_written(steps: Vec<Step>) -> Arc<MockClient> {
    let mut builder = MockClient::builder(
        settings(ReasoningLevel::Medium),
        Capabilities::new(CapabilitiesSpec::everything()).expect("capabilities"),
    );
    for step in steps {
        builder = builder.reply(Reply::success(step).build().expect("reply"));
    }
    Arc::new(builder.build())
}
