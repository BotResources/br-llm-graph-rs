//! What the runner call tests share: a schema with the keys a runner call
//! writes, the call of `documentalist` on the test port, and the graphs that
//! hold it.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use serde_json::{Map, Value as Json};

use crate::graph::{
    Always, Context, Graph, GraphBuilder, ItemFailure, Limit, Map as Each, Node, Target,
};
use crate::observe::{NoopObserver, Observer};
use crate::origin::{OccurrenceKey, Origin, RunId};
use crate::run::{Control, Outcome, RunFailure, control, run};
use crate::runner::{CallKey, RunnerArguments, RunnerCall, RunnerOutput, RunnerTarget};
use crate::state::{Config, Kind, Schema, State, StateKey, Value};
use crate::testkit::{SeqIds, TestRunners};
use crate::value::{EndLabel, Key, NodeId};

pub(crate) fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}

pub(crate) fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

pub(crate) fn skey<T: crate::state::ValueType>(name: &str) -> StateKey<T> {
    Schema::builder().state(name).unwrap()
}

pub(crate) fn target(name: &str) -> RunnerTarget {
    RunnerTarget::new(name).unwrap()
}

pub(crate) fn schema() -> Schema {
    Schema::builder()
        .state_by_name(key("question"), Kind::Str)
        .state_by_name(key("answer"), Kind::Str)
        .state_by_name(key("answers"), Kind::list(Kind::Str))
        .state_by_name(key("thread"), Kind::Str)
        .state_by_name(key("threads"), Kind::list(Kind::Str))
        .state_by_name(key("documents"), Kind::list(Kind::Str))
        .state_by_name(key("document"), Kind::Str)
        .state_by_name(key("failures"), Kind::list(Kind::Str))
        .state_by_name(key("count"), Kind::Int)
        .state_by_name(key("counts"), Kind::list(Kind::Int))
        .config_by_name(key("lang"), Kind::Str)
        .build()
}

/// The start state: `question`, `documents` and every other key empty.
pub(crate) fn state_with(question: &str, documents: &[&str]) -> State {
    let values = BTreeMap::from([
        (key("question"), Value::str(question)),
        (key("answer"), Value::str("")),
        (key("answers"), Value::list(Vec::new())),
        (key("thread"), Value::str("")),
        (key("threads"), Value::list(Vec::new())),
        (
            key("documents"),
            Value::list(documents.iter().copied().map(Value::str).collect()),
        ),
        (key("document"), Value::str("")),
        (key("failures"), Value::list(Vec::new())),
        (key("count"), Value::int(0)),
        (key("counts"), Value::list(Vec::new())),
    ]);
    State::new(schema(), values).unwrap()
}

pub(crate) fn config() -> Config {
    Config::new(&schema(), BTreeMap::from([(key("lang"), Value::str("fr"))])).unwrap()
}

/// The object `{"lang": "fr"}`.
pub(crate) fn in_french() -> Map<String, Json> {
    Map::from_iter([("lang".to_owned(), Json::from("fr"))])
}

pub(crate) fn set_answer() -> RunnerOutput {
    RunnerOutput::Set(skey("answer"))
}

pub(crate) fn append_answers() -> RunnerOutput {
    RunnerOutput::Append(skey("answers"))
}

/// The call of `documentalist` on `port` with `{"lang": "fr"}`, its answer
/// written through `output`, no thread written.
pub(crate) fn ask(port: &Arc<TestRunners>, output: RunnerOutput) -> RunnerCall {
    RunnerCall {
        target: target("documentalist"),
        arguments: RunnerArguments::value(in_french()),
        port: port.clone(),
        output,
        thread: None,
    }
}

/// The call of `documentalist` on `port` asking about the state's
/// `document`, its answer appended to `answers`.
pub(crate) fn ask_about_document(port: &Arc<TestRunners>) -> RunnerCall {
    let document: StateKey<String> = skey("document");
    RunnerCall {
        arguments: RunnerArguments::from_fn(move |state, _| {
            Ok(Map::from_iter([(
                "document".to_owned(),
                Json::from(state.get(&document)?),
            )]))
        }),
        ..ask(port, append_answers())
    }
}

/// A graph of `node` alone, as `ask`, ending `done`.
pub(crate) fn alone(node: impl Node + 'static) -> Graph {
    GraphBuilder::new(schema())
        .entry(nid("ask"))
        .node(nid("ask"), node)
        .edge(
            nid("ask"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap()
}

/// A graph mapping `body` over `documents`, each as `document`, as the map
/// `each`, at most `width` at once.
pub(crate) fn mapped(
    body: impl Node + 'static,
    width: Option<usize>,
    policy: ItemFailure,
) -> Graph {
    let mut each = Each::new(
        &skey::<Vec<String>>("documents"),
        &skey::<String>("document"),
        body,
    )
    .on_item_failure(policy);
    if let Some(width) = width {
        each = each.max_concurrency(Limit::Fixed(NonZeroUsize::new(width).unwrap()));
    }
    GraphBuilder::new(schema())
        .entry(nid("each"))
        .map(nid("each"), each)
        .edge(
            nid("each"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap()
}

/// The context of a run under the run id `run`, heard by `observer`.
pub(crate) fn ctx_heard(run: &str, observer: Arc<dyn Observer>) -> Context {
    Context::new(observer, Arc::new(SeqIds::new())).with_run_id(RunId::new(run))
}

/// The context of a run under the run id `run`.
pub(crate) fn ctx(run: &str) -> Context {
    ctx_heard(run, Arc::new(NoopObserver))
}

/// Runs `graph` on `state` in `ctx`, listening to `channel`.
pub(crate) async fn run_with(
    graph: &Graph,
    state: State,
    ctx: &Context,
    channel: &mut Control,
) -> Result<Outcome, RunFailure> {
    run(graph, &config(), state, None, ctx, channel).await
}

/// Runs `graph` on `state` in `ctx`.
pub(crate) async fn run_in(
    graph: &Graph,
    state: State,
    ctx: &Context,
) -> Result<Outcome, RunFailure> {
    let (_sender, mut channel) = control();
    run_with(graph, state, ctx, &mut channel).await
}

/// The final state of a finished run.
pub(crate) fn finished(result: Result<Outcome, RunFailure>) -> State {
    match result.map_err(|failure| failure.error).unwrap() {
        Outcome::Finished { state, .. } => state,
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

/// The texts of the list of strings `name` in `state`.
pub(crate) fn texts(state: &State, name: &str) -> Vec<String> {
    let list = state.get(&skey::<Vec<String>>(name)).unwrap();
    list.into_iter().map(str::to_owned).collect()
}

/// The key of the call made by the node at `occurrence` under `run`, for
/// the tool call `tool_call` when given.
pub(crate) fn call_key(run: &str, occurrence: &str, tool_call: Option<&str>) -> CallKey {
    CallKey::new(
        Origin {
            run: RunId::new(run),
            occurrence: OccurrenceKey::parse(occurrence).unwrap(),
        },
        tool_call.map(|id| br_llm_messages::ToolCallId::new(id).unwrap()),
    )
}
