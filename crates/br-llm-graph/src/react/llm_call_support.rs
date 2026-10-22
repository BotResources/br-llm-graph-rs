//! What the `LlmCall` tests share: a schema with string, list, conversation
//! and int keys, the summarizing call, and the graphs that hold it.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_clients_core::{
    ClientRequest, OutputMode, Parameter, ParameterKind, Parameters, StructuredShape,
};
use br_llm_clients_mock::{MockClient, Reply, ReplySource};
use br_llm_messages::{
    Author, Conversation, InputId, Text, Turn, TurnId, UserBlock, UserInput, UserSource,
    WireMessage, WireUserBlock,
};

use crate::error::GraphError;
use crate::graph::{
    Always, Context, Count, Graph, GraphBuilder, Limit, Map, Node, Target, TextOutput,
};
use crate::observe::NoopObserver;
use crate::react::given::{Model, Prompt};
use crate::react::llm_call::{LlmCall, LlmRequest};
use crate::react::test_support::*;
use crate::run::{Outcome, RunFailure, control, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::testkit::SeqIds;
use crate::value::{EndLabel, NodeId};

pub(crate) fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

pub(crate) fn schema() -> Schema {
    Schema::builder()
        .state_by_name(key("doc"), Kind::Str)
        .state_by_name(key("docs"), Kind::list(Kind::Str))
        .state_by_name(key("summary"), Kind::Str)
        .state_by_name(key("summaries"), Kind::list(Kind::Str))
        .state_by_name(key("chat"), Kind::Conversation)
        .state_by_name(key("count"), Kind::Int)
        .state_by_name(key("counts"), Kind::list(Kind::Int))
        .config_by_name(key("style"), Kind::Str)
        .build()
}

pub(crate) fn state_with(doc: &str, docs: &[&str]) -> State {
    let mut chat = Conversation::new();
    chat.push_input(
        UserInput::new(
            InputId::new("i-what-do-tides-follow").unwrap(),
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new("What do tides follow?").unwrap())],
        )
        .unwrap(),
    )
    .unwrap();
    chat.push_turn(Turn::new(
        TurnId::new("t0").unwrap(),
        Some(Author::new("librarian", "librarian").unwrap()),
        text_step("The moon, mostly."),
    ))
    .unwrap();
    let values = BTreeMap::from([
        (key("doc"), Value::str(doc)),
        (
            key("docs"),
            Value::list(docs.iter().copied().map(Value::str).collect()),
        ),
        (key("summary"), Value::str("")),
        (key("summaries"), Value::list(Vec::new())),
        (key("chat"), Value::conversation(chat)),
        (key("count"), Value::int(0)),
        (key("counts"), Value::list(Vec::new())),
    ]);
    State::new(schema(), values).unwrap()
}

pub(crate) fn config() -> Config {
    Config::new(
        &schema(),
        BTreeMap::from([(key("style"), Value::str("Be terse."))]),
    )
    .unwrap()
}

/// Summarizes the state key `doc` into `output`.
pub(crate) fn summarize(client: &MockClient, output: TextOutput) -> LlmCall {
    LlmCall {
        model: Model::client(Arc::new(client.clone())),
        system: Prompt::from_config(&ckey("style")),
        request: LlmRequest::Text(Prompt::from_state(&skey("doc"))),
        output_mode: OutputMode::Plain,
        output,
        reply_retries: None,
        reminder: None,
    }
}

/// A graph whose single node `node` ends `done`.
pub(crate) fn single(node: impl Node + 'static) -> Graph {
    GraphBuilder::new(schema())
        .entry(nid("call"))
        .node(nid("call"), node)
        .edge(
            nid("call"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap()
}

/// A graph mapping `body` over `docs`, at most `width` items at once.
pub(crate) fn mapped(body: impl Node + 'static, width: Option<usize>) -> Graph {
    let map = Map {
        list: key("docs"),
        item: key("doc"),
        body: Box::new(body),
        max_concurrency: width.map(|width| Limit::Fixed(NonZeroUsize::new(width).unwrap())),
        on_item_failure: Default::default(),
    };
    GraphBuilder::new(schema())
        .entry(nid("each"))
        .map(nid("each"), map)
        .edge(
            nid("each"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap()
}

pub(crate) async fn run_in(
    graph: &Graph,
    state: State,
    ctx: &Context,
) -> Result<Outcome, RunFailure> {
    let (_sender, mut channel) = control();
    run(graph, &config(), state, None, ctx, &mut channel).await
}

pub(crate) fn ctx() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

pub(crate) fn finished(result: Result<Outcome, RunFailure>) -> State {
    match result.map_err(|failure| failure.error).unwrap() {
        Outcome::Finished { state, .. } => state,
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

pub(crate) fn user_text(text: &str) -> WireMessage {
    WireMessage::User {
        content: vec![WireUserBlock::Text {
            text: Text::new(text).unwrap(),
        }],
    }
}

/// The output setting `summary`.
pub(crate) fn set_summary() -> TextOutput {
    TextOutput::Set(skey("summary"))
}

/// The output appending to `summaries`.
pub(crate) fn append_summaries() -> TextOutput {
    TextOutput::Append(skey("summaries"))
}

/// The text of the first user message of `request`.
pub(crate) fn asked(request: &ClientRequest) -> String {
    match request.messages().first() {
        Some(WireMessage::User { content }) => content
            .iter()
            .map(|block| match block {
                WireUserBlock::Text { text } => text.as_str().to_owned(),
                _ => String::new(),
            })
            .collect(),
        other => panic!("expected a user message first, got {other:?}"),
    }
}

/// A reply function answering `summary of {asked text}`.
pub(crate) fn echoing() -> ReplySource {
    ReplySource::function(|call| {
        Reply::success(text_step(&format!("summary of {}", asked(call.request())))).build()
    })
}

/// The shape `facts`: `subject` (string) and `count` (integer), both
/// required.
pub(crate) fn facts_mode() -> OutputMode {
    let parameters = Parameters::new(vec![
        Parameter::new("subject", "", true, ParameterKind::String).unwrap(),
        Parameter::new("count", "", true, ParameterKind::Integer).unwrap(),
    ])
    .unwrap();
    OutputMode::Structured(StructuredShape::new("facts", None, parameters).unwrap())
}

/// The summarizing call on `client`, asking for `facts`, with `retries`.
pub(crate) fn asking_facts(client: &MockClient, retries: Option<Count>) -> LlmCall {
    LlmCall {
        output_mode: facts_mode(),
        reply_retries: retries,
        ..summarize(client, set_summary())
    }
}

/// The reminder the call sends by default after a reply refused for
/// `reason`.
pub(crate) fn reminded_of(reason: &str) -> br_llm_messages::WireMessage {
    user_text(&format!(
        "Your reply could not be used: {reason}. Reply again."
    ))
}

/// The summary a finished run wrote.
pub(crate) async fn summary_of(call: LlmCall) -> Result<String, GraphError> {
    run_in(&single(call), state_with("doc", &[]), &ctx())
        .await
        .map(|outcome| {
            finished(Ok(outcome))
                .str(&key("summary"))
                .unwrap()
                .to_owned()
        })
        .map_err(|failure| failure.error)
}
