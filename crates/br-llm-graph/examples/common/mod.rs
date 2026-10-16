//! What the examples share: an observer that prints, an id source, a thread
//! store kept in memory, scripted clients and the messages they answer.

pub mod store;
pub mod tools;
pub mod turn;

use std::sync::Arc;

use br_llm_clients_core::{
    CacheRequirement, ClientError, ClientSettings, MaxOutputTokens, ReasoningLevel,
};
use br_llm_clients_mock::{Capabilities, CapabilitiesSpec, MockClient, Reply};
use br_llm_graph::{
    ConfigKey, Context, Cursor, EndLabel, IdRequest, IdSource, Key, NoopObserver, Observer, Origin,
    Schema, State, StateKey, ThreadId, Update, ValueType,
};
use br_llm_messages::{
    AssistantBlock, Conversation, InputId, ModelId, Step, StopReason, StreamEvent, Text, ToolCall,
    ToolCallId, ToolName, TurnId, UserBlock, UserInput, UserSource,
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

/// An id source that writes each id from its request, so the same request
/// always gives the same id: the turn id names the request, the thread id is
/// a UUIDv7 made from a hash of it.
pub struct SeqIds;

impl SeqIds {
    pub fn new() -> Self {
        Self
    }
}

/// The request written as text: run, occurrence, trigger and rank.
fn request_text(request: &IdRequest) -> String {
    let trigger = request.trigger.as_ref().map_or("-", InputId::as_str);
    format!(
        "{}/{}/{trigger}/{}",
        request.origin.run, request.origin.occurrence, request.rank
    )
}

impl IdSource for SeqIds {
    fn thread_id(&self, request: &IdRequest) -> ThreadId {
        use std::hash::{Hash, Hasher};
        let text = request_text(request);
        let mut high = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut high);
        let mut low = std::collections::hash_map::DefaultHasher::new();
        (&text, 1u8).hash(&mut low);
        let (high, low) = (high.finish(), low.finish());
        ThreadId::new(format!(
            "{:08x}-{:04x}-7{:03x}-{:x}{:03x}-{:012x}",
            high >> 32,
            (high >> 16) & 0xffff,
            high & 0xfff,
            8 + ((low >> 62) & 0x3),
            (low >> 48) & 0xfff,
            low & 0xffff_ffff_ffff
        ))
        .expect("a UUIDv7 text")
    }

    fn turn_id(&self, request: &IdRequest) -> TurnId {
        TurnId::new(format!("turn/{}", request_text(request))).expect("non-empty turn id")
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

/// The typed key of the state key `name` of a graph the example did not
/// declare itself, such as an agent's (`opening`, `thread`, `answer`): a
/// typed key is a name and the Rust type of its kind.
pub fn state_key<T: ValueType>(name: &str) -> StateKey<T> {
    Schema::builder().state(name).expect("valid key")
}

/// The typed key of the configuration key `name` of a graph the example did
/// not declare itself.
pub fn config_key<T: ValueType>(name: &str) -> ConfigKey<T> {
    Schema::builder().config(name).expect("valid key")
}

/// A conversation of `inputs`, in order: an agent's `opening`.
pub fn opening(inputs: impl IntoIterator<Item = UserInput>) -> Conversation {
    let mut conversation = Conversation::new();
    for input in inputs {
        conversation
            .push_input(input)
            .expect("an input id unique in the conversation");
    }
    conversation
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

/// A client whose first reply streams `after` events of `cut` then fails
/// (`Unavailable`), then answers `steps` in order: a provider that drops a
/// connection in the middle of a reply.
pub fn scripted_cut(cut: Step, after: usize, steps: Vec<Step>) -> Arc<MockClient> {
    let mut builder = MockClient::builder(
        settings(ReasoningLevel::Medium),
        Capabilities::new(CapabilitiesSpec::everything()).expect("capabilities"),
    )
    .reply(
        Reply::cut(cut, after, ClientError::Unavailable { retry_after: None })
            .rename_tool_call_ids("cut")
            .build()
            .expect("reply"),
    );
    for step in steps {
        builder = builder.reply(
            Reply::success(step)
                .rename_tool_call_ids("call")
                .build()
                .expect("reply"),
        );
    }
    Arc::new(builder.build())
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
