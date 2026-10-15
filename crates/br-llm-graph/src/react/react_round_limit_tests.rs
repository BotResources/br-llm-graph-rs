use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use br_llm_clients_mock::MockClient;
use br_llm_messages::{
    AssistantBlock, Conversation, Entry, StopReason, Text, ToolResultBlock, Turn, TurnId, TurnItem,
    TurnState, UserBlock, UserInput, UserSource,
};
use serde_json::json;

use crate::error::GraphError;
use crate::graph::{Context, Graph, GraphBuilder, Limit, Target};
use crate::observe::NoopObserver;
use crate::react::llm_node::LlmNode;
use crate::react::react_loop::ReactLoop;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::source::Source;
use crate::react::test_support::{
    EchoTool, author, key, mock, requests, text_step, tool_call_step, tool_names,
};
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

/// The tools each call of `client` declared, in call order.
fn declared(client: &MockClient) -> Vec<Vec<String>> {
    requests(client).iter().map(tool_names).collect()
}

fn graph(client: &MockClient, max_rounds: Limit, on_limit: OnLimit) -> Graph {
    let llm = LlmNode {
        key: key("chat"),
        author: author(),
        client: Arc::new(client.clone()),
        system: vec![Source::Config(key("base"))],
        tools: vec![Arc::new(EchoTool)],
        enabled: None,
        output: OutputMode::Plain,
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
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
        tool_call_step("c3", "echo", json!({})),
    ]);
    let graph = graph(&client, two(), OnLimit::Error);

    let failure = run_graph(&graph, config(2), fresh_state())
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure.error,
        GraphError::ToolLimitReached { ref node, max_rounds: 2 } if *node == nid("llm")
    ));
    assert_eq!(declared(&client), vec![vec!["echo".to_owned()]; 3]);
    assert_eq!(result_texts(&own_turn(&failure.checkpoint.state)).len(), 2);
}

#[tokio::test]
async fn given_continue_limit_when_reached_then_calls_refused_and_one_last_call_still_declares_the_tools()
 {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
        text_step("final answer"),
    ]);
    let graph = graph(&client, Limit::Config(key("max_rounds")), continue_limit());

    let state = finished(run_graph(&graph, config(1), fresh_state()).await);
    assert_eq!(
        declared(&client),
        vec![vec!["echo".to_owned()]; 3],
        "the call after the limit declares the tools like the others"
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
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
        tool_call_step("c3", "echo", json!({})),
    ]);
    let graph = graph(&client, Limit::Config(key("max_rounds")), continue_limit());

    let failure = run_graph(&graph, config(1), fresh_state())
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure.error,
        GraphError::ToolLimitReached { max_rounds: 1, .. }
    ));
    assert_eq!(
        client.calls(),
        3,
        "one model call after the limit, then the run fails"
    );
    assert_eq!(
        declared(&client).last(),
        Some(&vec!["echo".to_owned()]),
        "the last call still declared the tools"
    );
    assert!(
        failure.checkpoint.state.bool(&key("limited")).unwrap(),
        "the limit node ran and set the flag before the last call"
    );
}

#[tokio::test]
async fn given_end_limit_when_reached_then_calls_refused_and_turn_closed_without_a_model_call() {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
    ]);
    let on_limit = OnLimit::End {
        node: nid("limit"),
        flag: key("limited"),
    };
    let graph = graph(&client, Limit::Config(key("max_rounds")), on_limit);

    let state = finished(run_graph(&graph, config(1), fresh_state()).await);
    assert_eq!(client.calls(), 2, "the turn is closed without a model call");
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
async fn given_a_reply_after_the_last_round_that_calls_an_unknown_tool_when_run_then_the_limit_applies_first()
 {
    let steps = || {
        vec![
            tool_call_step("c1", "echo", json!({})),
            tool_call_step("c2", "missing", json!({})),
        ]
    };
    let one = || Limit::Fixed(NonZeroUsize::new(1).unwrap());
    let failure = run_graph(
        &graph(&mock(steps()), one(), OnLimit::Error),
        config(1),
        fresh_state(),
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(failure.error, GraphError::ToolLimitReached { .. }));
    let on_limit = OnLimit::End {
        node: nid("limit"),
        flag: key("limited"),
    };
    let state = finished(
        run_graph(
            &graph(&mock(steps()), one(), on_limit),
            config(1),
            fresh_state(),
        )
        .await,
    );
    assert!(state.bool(&key("limited")).unwrap());
    assert!(result_texts(&own_turn(&state))[1].1);
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
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("second answer"),
    ]);
    let graph = graph(&client, Limit::Config(key("max_rounds")), continue_limit());

    let state = finished(run_graph(&graph, config(1), state_with(chat)).await);
    assert_eq!(client.calls(), 2);
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
            client: Arc::new(mock(Vec::new())),
            system: Vec::new(),
            tools: vec![Arc::new(EchoTool)],
            enabled: None,
            output: OutputMode::Plain,
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
