use std::collections::BTreeMap;

use br_llm_clients_core::{
    CacheRequirement, ClientError, ClientRequest, ClientSettings, MaxOutputTokens, ReasoningLevel,
};
use br_llm_clients_mock::{Capabilities, CapabilitiesSpec, MockClient, Reply};
use br_llm_messages::{
    AssistantBlock, Author, Conversation, InputId, ModelId, Step, StopReason, Text, ToolCall,
    ToolCallId, ToolName, ToolResultBlock, UserBlock, UserInput, UserSource,
};
use serde_json::{Value as Json, json};

use crate::react::given::{ModelNode, Prompt};
pub(crate) use crate::react::test_clients::{LaxClient, StreamRecorder};
pub(crate) use crate::react::test_tools::{
    EchoTool, ScopeTool, StrictTool, TallyTool, WriteFileTool, spec,
};
use crate::state::{Config, ConfigKey, Kind, Schema, State, StateKey, Value, ValueType};
use crate::value::Key;

pub(crate) fn author() -> Author {
    Author::new("agent", "agent").unwrap()
}

pub(crate) fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}

/// The typed state key `name`, of the kind of `T`.
pub(crate) fn skey<T: ValueType>(name: &str) -> StateKey<T> {
    Schema::builder().state(name).unwrap()
}

/// The typed configuration key `name`, of the kind of `T`.
pub(crate) fn ckey<T: ValueType>(name: &str) -> ConfigKey<T> {
    Schema::builder().config(name).unwrap()
}

/// The text of the configuration key `base`, the agent's usual system text.
pub(crate) fn base<N: ModelNode>() -> Prompt<N> {
    Prompt::from_config(&ckey("base"))
}

pub(crate) fn schema() -> Schema {
    Schema::builder()
        .state_by_name(key("chat"), Kind::Conversation)
        .state_by_name(key("log"), Kind::list(Kind::Str))
        .state_by_name(key("enabled"), Kind::list(Kind::Str))
        .config_by_name(key("base"), Kind::Str)
        .build()
}

pub(crate) fn seeded_state() -> State {
    let mut chat = Conversation::new();
    chat.push_input(
        UserInput::new(
            InputId::new("i-hi").unwrap(),
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new("hi").unwrap())],
        )
        .unwrap(),
    )
    .unwrap();
    let mut values = BTreeMap::new();
    values.insert(key("chat"), Value::conversation(chat));
    values.insert(key("log"), Value::list(Vec::new()));
    values.insert(
        key("enabled"),
        Value::list(vec![Value::str("echo"), Value::str("tally")]),
    );
    State::new(schema(), values).unwrap()
}

pub(crate) fn config() -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("base"), Value::str("You are a helpful agent."));
    Config::new(&schema(), values).unwrap()
}

pub(crate) fn text_step(body: &str) -> Step {
    Step::new(
        vec![AssistantBlock::Text {
            text: Text::new(body).unwrap(),
        }],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap()
}

pub(crate) fn tool_call_step(id: &str, name: &str, arguments: Json) -> Step {
    Step::new(
        vec![AssistantBlock::ToolCall(ToolCall {
            id: ToolCallId::new(id).unwrap(),
            name: ToolName::new(name).unwrap(),
            arguments,
        })],
        StopReason::AwaitingToolResults,
        None,
        None,
    )
    .unwrap()
}

/// A step calling each named tool once, with ids `c1`, `c2`, ... (a mock
/// renames them when it serves the step).
pub(crate) fn calls_step(names: &[&str]) -> Step {
    Step::new(
        names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                AssistantBlock::ToolCall(ToolCall {
                    id: ToolCallId::new(format!("c{}", index + 1)).unwrap(),
                    name: ToolName::new(*name).unwrap(),
                    arguments: json!({}),
                })
            })
            .collect(),
        StopReason::AwaitingToolResults,
        None,
        None,
    )
    .unwrap()
}

/// Each tool result of a turn as `(tool, is_error, text)`, in order.
pub(crate) fn results_of(turn: &br_llm_messages::Turn) -> Vec<(String, bool, String)> {
    turn.items()
        .iter()
        .filter_map(|item| match item {
            br_llm_messages::TurnItem::ToolResults(results) => Some(results.results()),
            br_llm_messages::TurnItem::Step(_) => None,
        })
        .flatten()
        .map(|result| {
            let text = result
                .content
                .iter()
                .map(|block| match block {
                    ToolResultBlock::Text { text } => text.as_str().to_owned(),
                    _ => String::new(),
                })
                .collect::<Vec<_>>()
                .join(" ");
            (result.tool_name.as_str().to_owned(), result.is_error, text)
        })
        .collect()
}

pub(crate) fn structured_step(value: Json) -> Step {
    Step::new(
        vec![AssistantBlock::Structured { value }],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap()
}

/// A step holding only a thinking block: a reply a conforming client refuses
/// (`ReplyUnusable`), served only by `LaxClient`.
pub(crate) fn thinking_only_step() -> Step {
    Step::new(
        vec![AssistantBlock::Thinking(br_llm_messages::Thinking {
            text: "hmm".to_owned(),
            signature: None,
        })],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap()
}

/// The settings of a test client, at `reasoning`.
pub(crate) fn settings(reasoning: ReasoningLevel) -> ClientSettings {
    ClientSettings::new(
        ModelId::new("mock-model").unwrap(),
        reasoning,
        CacheRequirement::NotRequired,
        MaxOutputTokens::new(4096).unwrap(),
    )
}

/// A mock that can do everything, at `reasoning`, with an empty script.
pub(crate) fn mock_at(reasoning: ReasoningLevel) -> MockClient {
    MockClient::new(
        settings(reasoning),
        Capabilities::new(CapabilitiesSpec::everything()).unwrap(),
    )
}

/// A mock (reasoning `Medium`) answering `steps` in order, the tool call ids
/// of each renamed when it is served (`call-{seq}-{i}`), so a script can call
/// tools again in one conversation.
pub(crate) fn mock(steps: Vec<Step>) -> MockClient {
    let mock = mock_at(ReasoningLevel::Medium);
    for step in steps {
        push(&mock, step);
    }
    mock
}

/// Adds `step` at the end of the script of `mock`, its tool call ids renamed
/// when it is served.
pub(crate) fn push(mock: &MockClient, step: Step) {
    mock.push_reply(
        Reply::success(step)
            .rename_tool_call_ids("call")
            .build()
            .unwrap(),
    );
}

/// A mock whose next call fails with `error`.
pub(crate) fn failing(error: ClientError) -> MockClient {
    let mock = mock_at(ReasoningLevel::Medium);
    mock.push_reply(Reply::failure(error).build().unwrap());
    mock
}

/// The requests the calls of `mock` received, in call order.
pub(crate) fn requests(mock: &MockClient) -> Vec<ClientRequest> {
    mock.log()
        .into_iter()
        .map(|call| call.request().clone())
        .collect()
}

/// The system text of `request`, if any.
pub(crate) fn system_of(request: &ClientRequest) -> Option<String> {
    request.system().map(|text| text.as_str().to_owned())
}

/// The names of the tools `request` declares, in order.
pub(crate) fn tool_names(request: &ClientRequest) -> Vec<String> {
    request
        .tools()
        .iter()
        .map(|spec| spec.name().as_str().to_owned())
        .collect()
}

/// The error a node returned, read back as a `GraphError`.
pub(crate) fn returned(error: &crate::error::GraphError) -> &crate::error::GraphError {
    match error {
        crate::error::GraphError::NodeFailed {
            source: crate::error::NodeFault::Returned(returned),
            ..
        } => returned
            .downcast_ref::<crate::error::GraphError>()
            .expect("the node returned a GraphError"),
        other => panic!("expected a node that returned an error, got: {other}"),
    }
}
