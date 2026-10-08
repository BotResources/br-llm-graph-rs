use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use br_llm_messages::{
    AssistantBlock, Conversation, Entry, StopReason, Text, ToolResultBlock, Turn, TurnId, TurnItem,
    TurnState, UserBlock, UserInput, UserSource,
};
use serde_json::json;

use crate::error::GraphError;
use crate::graph::{Context, Graph, GraphBuilder, Limit, Target};
use crate::observe::NoopObserver;
use crate::react::llm_node::{LlmNode, Source};
use crate::react::model::{Model, ModelFuture, OutputMode, Request, StreamSink, ToolCalls};
use crate::react::react_loop::ReactLoop;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::test_support::{EchoTool, author, key, text_step, tool_call_step};
use crate::run::{Outcome, RunFailure, channel, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::testkit::SeqIds;
use crate::value::{EndLabel, NodeId};

fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn schema() -> Schema {
    Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .state(key("log"), Kind::list(Kind::Str))
        .state(key("limited"), Kind::Bool)
        .config(key("base"), Kind::Str)
        .config(key("max_rounds"), Kind::Int)
        .build()
}

fn config(max_rounds: i64) -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("base"), Value::str("You are a helpful agent."));
    values.insert(key("max_rounds"), Value::int(max_rounds));
    Config::new(&schema(), values).unwrap()
}

fn human(body: &str) -> UserInput {
    UserInput::new(
        UserSource::Human,
        None,
        vec![UserBlock::text(Text::new(body).unwrap())],
    )
    .unwrap()
}

fn state_with(chat: Conversation) -> State {
    let mut values = BTreeMap::new();
    values.insert(key("chat"), Value::conversation(chat));
    values.insert(key("log"), Value::list(Vec::new()));
    values.insert(key("limited"), Value::bool(false));
    State::new(schema(), values).unwrap()
}

fn fresh_state() -> State {
    let mut chat = Conversation::new();
    chat.push_input(human("hi"));
    state_with(chat)
}

struct RecordingModel {
    steps: Mutex<VecDeque<br_llm_messages::Step>>,
    seen: Mutex<Vec<ToolCalls>>,
}

impl RecordingModel {
    fn new(steps: Vec<br_llm_messages::Step>) -> Arc<Self> {
        Arc::new(Self {
            steps: Mutex::new(steps.into_iter().collect()),
            seen: Mutex::new(Vec::new()),
        })
    }

    fn seen(&self) -> Vec<ToolCalls> {
        self.seen.lock().unwrap().clone()
    }
}

impl Model for RecordingModel {
    fn complete<'a>(&'a self, request: Request, _sink: &'a dyn StreamSink) -> ModelFuture<'a> {
        self.seen.lock().unwrap().push(request.tool_calls);
        let step = self.steps.lock().unwrap().pop_front();
        Box::pin(async move { step.ok_or_else(|| "script exhausted".into()) })
    }
}

fn graph(model: Arc<RecordingModel>, max_rounds: Limit, on_limit: OnLimit) -> Graph {
    let llm = LlmNode {
        key: key("chat"),
        author: author(),
        model,
        system: vec![Source::Config(key("base"))],
        tools: vec![Arc::new(EchoTool)],
        enabled: None,
        output: OutputMode::Text,
    };
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![(nid("tools"), vec![Arc::new(EchoTool)])],
        after: Target::End(EndLabel::new("done").unwrap()),
        tool_concurrency: None,
        round_limit: Some(RoundLimit {
            max_rounds,
            on_limit,
        }),
    };
    react
        .add(GraphBuilder::new(schema()).entry(nid("llm")), llm)
        .unwrap()
        .build()
        .unwrap()
}

async fn run_graph(graph: &Graph, config: Config, state: State) -> Result<Outcome, RunFailure> {
    let ctx = Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()));
    let (_sender, mut inbox) = channel();
    run(graph, &config, state, None, &ctx, &mut inbox).await
}

fn finished(result: Result<Outcome, RunFailure>) -> State {
    match result.map_err(|f| f.error).unwrap() {
        Outcome::Finished { state, .. } => state,
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

fn own_turn(state: &State) -> Turn {
    match state.conversation(&key("chat")).unwrap().entries().last() {
        Some(Entry::Turn(turn)) => turn.clone(),
        Some(Entry::UserInput(_)) | None => panic!("expected the agent's turn last"),
    }
}

fn result_texts(turn: &Turn) -> Vec<(String, bool)> {
    let mut texts = Vec::new();
    for item in turn.items() {
        if let TurnItem::ToolResults(results) = item {
            for result in results.results() {
                for block in &result.content {
                    let ToolResultBlock::Text { text } = block else {
                        continue;
                    };
                    texts.push((text.as_str().to_owned(), result.is_error));
                }
            }
        }
    }
    texts
}

fn continue_limit() -> OnLimit {
    OnLimit::Continue {
        node: nid("limit"),
        flag: key("limited"),
    }
}

fn two() -> Limit {
    Limit::Fixed(NonZeroUsize::new(2).unwrap())
}

#[tokio::test]
async fn given_error_limit_when_model_keeps_calling_then_run_fails_after_the_allowed_rounds() {
    let model = RecordingModel::new(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
        tool_call_step("c3", "echo", json!({})),
    ]);
    let graph = graph(model.clone(), two(), OnLimit::Error);

    let failure = run_graph(&graph, config(2), fresh_state())
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure.error,
        GraphError::ToolLimitReached { ref node, max_rounds: 2 } if *node == nid("llm")
    ));
    assert_eq!(model.seen(), vec![ToolCalls::Allowed; 3]);
    assert_eq!(result_texts(&own_turn(&failure.checkpoint.state)).len(), 2);
}

#[tokio::test]
async fn given_continue_limit_when_reached_then_calls_refused_and_one_last_call_forbids_tools() {
    let model = RecordingModel::new(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
        text_step("final answer"),
    ]);
    let graph = graph(
        model.clone(),
        Limit::Config(key("max_rounds")),
        continue_limit(),
    );

    let state = finished(run_graph(&graph, config(1), fresh_state()).await);
    assert_eq!(
        model.seen(),
        vec![ToolCalls::Allowed, ToolCalls::Allowed, ToolCalls::Forbidden]
    );
    assert!(state.bool(&key("limited")).unwrap());
    let turn = own_turn(&state);
    assert!(matches!(
        turn.state(),
        TurnState::Finished {
            stop_reason: StopReason::EndTurn
        }
    ));
    let texts = result_texts(&turn);
    assert_eq!(texts[0], ("echoed".to_owned(), false));
    assert!(
        texts[1]
            .0
            .starts_with("Tool round limit reached (1): this call was not executed.")
    );
    assert!(texts[1].1);
}

#[tokio::test]
async fn given_continue_limit_when_the_last_reply_still_calls_tools_then_run_fails() {
    let model = RecordingModel::new(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
        tool_call_step("c3", "echo", json!({})),
    ]);
    let graph = graph(
        model.clone(),
        Limit::Config(key("max_rounds")),
        continue_limit(),
    );

    let failure = run_graph(&graph, config(1), fresh_state())
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure.error,
        GraphError::ToolLimitReached { max_rounds: 1, .. }
    ));
    assert_eq!(model.seen().last(), Some(&ToolCalls::Forbidden));
}

#[tokio::test]
async fn given_end_limit_when_reached_then_calls_refused_and_turn_closed_without_a_model_call() {
    let model = RecordingModel::new(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
    ]);
    let on_limit = OnLimit::End {
        node: nid("limit"),
        flag: key("limited"),
    };
    let graph = graph(model.clone(), Limit::Config(key("max_rounds")), on_limit);

    let state = finished(run_graph(&graph, config(1), fresh_state()).await);
    assert_eq!(model.seen().len(), 2);
    assert!(state.bool(&key("limited")).unwrap());
    let turn = own_turn(&state);
    let TurnState::Finished {
        stop_reason: StopReason::Other { reason },
    } = turn.state()
    else {
        panic!("expected a turn closed by the limit");
    };
    assert_eq!(reason, "tool_round_limit");
    assert!(result_texts(&turn)[1].1);
}

#[tokio::test]
async fn given_an_earlier_finished_turn_when_the_agent_runs_again_then_rounds_restart_from_zero() {
    let mut earlier = Turn::new(
        TurnId::new("t0").unwrap(),
        Some(author()),
        tool_call_step("e1", "echo", json!({})),
    );
    earlier
        .push_result(br_llm_messages::ToolResult::new(
            br_llm_messages::ToolCallId::new("e1").unwrap(),
            br_llm_messages::ToolName::new("echo").unwrap(),
            vec![ToolResultBlock::text(Text::new("echoed").unwrap())],
            false,
        ))
        .unwrap();
    earlier.push_step(text_step("earlier answer")).unwrap();
    let mut chat = Conversation::new();
    chat.push_input(human("hi"));
    chat.push_turn(earlier).unwrap();
    chat.push_input(human("again"));
    let model = RecordingModel::new(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("second answer"),
    ]);
    let graph = graph(
        model.clone(),
        Limit::Config(key("max_rounds")),
        continue_limit(),
    );

    let state = finished(run_graph(&graph, config(1), state_with(chat)).await);
    assert_eq!(model.seen(), vec![ToolCalls::Allowed; 2]);
    assert!(!state.bool(&key("limited")).unwrap());
    assert!(matches!(
        own_turn(&state).items().last(),
        Some(TurnItem::Step(step)) if matches!(step.content(), [AssistantBlock::Text { .. }])
    ));
}

#[test]
fn given_a_flag_or_limit_key_of_the_wrong_kind_when_added_then_refused() {
    let add = |max_rounds: Limit, flag: &str| {
        let llm = LlmNode {
            key: key("chat"),
            author: author(),
            model: RecordingModel::new(Vec::new()),
            system: Vec::new(),
            tools: vec![Arc::new(EchoTool)],
            enabled: None,
            output: OutputMode::Text,
        };
        ReactLoop {
            llm: nid("llm"),
            tool_nodes: vec![(nid("tools"), vec![Arc::new(EchoTool)])],
            after: Target::End(EndLabel::new("done").unwrap()),
            tool_concurrency: None,
            round_limit: Some(RoundLimit {
                max_rounds,
                on_limit: OnLimit::Continue {
                    node: nid("limit"),
                    flag: key(flag),
                },
            }),
        }
        .add(GraphBuilder::new(schema()).entry(nid("llm")), llm)
        .err()
    };
    assert!(matches!(
        add(two(), "log"),
        Some(GraphError::FlagKeyMismatch { .. })
    ));
    assert!(matches!(
        add(Limit::Config(key("base")), "limited"),
        Some(GraphError::LimitKeyMismatch { .. })
    ));
}
