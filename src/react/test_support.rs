use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_llm_messages::{
    AssistantBlock, Author, Conversation, Step, StopReason, StreamEvent, Text, ToolCall,
    ToolCallId, ToolName, ToolResultBlock, UserBlock, UserInput, UserSource,
};
use serde_json::{Value as Json, json};

use crate::react::model::{Model, ModelFuture, StreamSink, ToolSpec};
use crate::react::tool::{Tool, ToolFuture, ToolOutput};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;
use crate::value::Key;

pub(crate) fn author() -> Author {
    Author::new("agent").unwrap()
}

pub(crate) fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}

pub(crate) fn schema() -> Schema {
    Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .state(key("log"), Kind::list(Kind::Str))
        .state(key("enabled"), Kind::list(Kind::Str))
        .config(key("base"), Kind::Str)
        .build()
}

pub(crate) fn seeded_state() -> State {
    let mut chat = Conversation::new();
    chat.push_input(
        UserInput::new(
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new("hi").unwrap())],
        )
        .unwrap(),
    );
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

pub(crate) fn structured_step(value: Json) -> Step {
    Step::new(
        vec![AssistantBlock::Structured { value }],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap()
}

pub(crate) struct ScriptedModel {
    steps: Mutex<VecDeque<Step>>,
}

impl ScriptedModel {
    pub(crate) fn new(steps: Vec<Step>) -> Self {
        Self {
            steps: Mutex::new(steps.into_iter().collect()),
        }
    }
}

impl Model for ScriptedModel {
    fn complete<'a>(
        &'a self,
        _request: crate::react::model::Request,
        sink: &'a dyn StreamSink,
    ) -> ModelFuture<'a> {
        let step = self
            .steps
            .lock()
            .ok()
            .and_then(|mut queue| queue.pop_front());
        Box::pin(async move {
            let step = step.ok_or_else(|| -> crate::react::model::ModelError {
                "scripted model exhausted".into()
            })?;
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

pub(crate) struct FailingModel;

impl Model for FailingModel {
    fn complete<'a>(
        &'a self,
        _request: crate::react::model::Request,
        _sink: &'a dyn StreamSink,
    ) -> ModelFuture<'a> {
        Box::pin(async { Err("boom".into()) })
    }
}

pub(crate) struct RecordingModel {
    pub(crate) tools_seen: Mutex<Vec<String>>,
    pub(crate) system_seen: Mutex<Option<String>>,
}

impl RecordingModel {
    pub(crate) fn new() -> Self {
        Self {
            tools_seen: Mutex::new(Vec::new()),
            system_seen: Mutex::new(None),
        }
    }
}

impl Model for RecordingModel {
    fn complete<'a>(
        &'a self,
        request: crate::react::model::Request,
        _sink: &'a dyn StreamSink,
    ) -> ModelFuture<'a> {
        if let Ok(mut seen) = self.tools_seen.lock() {
            *seen = request
                .tools
                .iter()
                .map(|t| t.name.as_str().to_owned())
                .collect();
        }
        if let Ok(mut seen) = self.system_seen.lock() {
            *seen = request.system.clone();
        }
        Box::pin(async { Ok(text_step("recorded")) })
    }
}

fn spec(name: &str) -> ToolSpec {
    ToolSpec {
        name: ToolName::new(name).unwrap(),
        description: format!("the {name} tool"),
        parameters: json!({ "type": "object" }),
    }
}

pub(crate) struct EchoTool;

impl Tool for EchoTool {
    fn spec(&self) -> ToolSpec {
        spec("echo")
    }
    fn safe(&self) -> bool {
        true
    }
    fn call<'a>(
        &'a self,
        _arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("echoed").unwrap(),
            )]))
        })
    }
}

pub(crate) struct TallyTool;

impl Tool for TallyTool {
    fn spec(&self) -> ToolSpec {
        spec("tally")
    }
    fn safe(&self) -> bool {
        true
    }
    fn call<'a>(
        &'a self,
        _arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(
                ToolOutput::text(vec![ToolResultBlock::text(Text::new("tallied").unwrap())])
                    .with_updates(vec![Update::Append {
                        key: key("log"),
                        value: Value::str("tallied"),
                    }]),
            )
        })
    }
}

#[derive(Default)]
pub(crate) struct WriteFileTool {
    pub(crate) calls: AtomicUsize,
}

impl Tool for WriteFileTool {
    fn spec(&self) -> ToolSpec {
        spec("write_file")
    }
    fn safe(&self) -> bool {
        false
    }
    fn call<'a>(
        &'a self,
        _arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("written").unwrap(),
            )]))
        })
    }
}

pub(crate) struct StrictTool;

impl Tool for StrictTool {
    fn spec(&self) -> ToolSpec {
        spec("strict")
    }
    fn safe(&self) -> bool {
        true
    }
    fn call<'a>(
        &'a self,
        arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async move {
            let _parsed: StrictArgs = serde_json::from_value(arguments)?;
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("ok").unwrap(),
            )]))
        })
    }
}

#[derive(serde::Deserialize)]
struct StrictArgs {
    #[allow(dead_code)]
    n: i64,
}
