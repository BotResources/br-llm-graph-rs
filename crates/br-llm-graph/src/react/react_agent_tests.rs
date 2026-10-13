use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_messages::{
    Conversation, Entry, Text, ToolCallId, ToolName, ToolResult, ToolResultBlock, Turn, TurnId,
    TurnItem, UserBlock, UserInput, UserSource,
};
use serde_json::json;

use crate::error::GraphError;
use crate::graph::{
    Always, Context, Graph, GraphBuilder, Input, ItemFailure, Limit, Map, Output, SubGraph, Switch,
    Target,
};
use crate::observe::NoopObserver;
use crate::react::llm_node::Source;
use crate::react::model::OutputMode;
use crate::react::react_agent::ReactAgent;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::test_support::*;
use crate::run::{Outcome, RunFailure, channel, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::testkit::SeqIds;
use crate::value::{EndLabel, NodeId};

fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn question(body: &str) -> Conversation {
    let mut chat = Conversation::new();
    chat.push_input(
        UserInput::new(
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new(body).unwrap())],
        )
        .unwrap(),
    );
    chat
}

fn agent(model: Arc<LoggingModel>) -> ReactAgent {
    ReactAgent {
        author: author(),
        model,
        system: vec![Source::Config(key("base"))],
        output: OutputMode::Text,
        tools: vec![Arc::new(EchoTool)],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        thinking: None,
        enabled: None,
        inputs: Vec::new(),
    }
}

fn limited(model: Arc<LoggingModel>, max_rounds: usize, on_limit: OnLimit) -> ReactAgent {
    ReactAgent {
        round_limit: Some(RoundLimit {
            max_rounds: Limit::Fixed(NonZeroUsize::new(max_rounds).unwrap()),
            on_limit,
        }),
        ..agent(model)
    }
}

fn agent_config(graph: &Graph) -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("base"), Value::str("You are a helpful agent."));
    Config::new(graph.schema(), values).unwrap()
}

fn ctx() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

async fn call(graph: &Graph, history: Conversation) -> Result<Outcome, RunFailure> {
    call_in(graph, history, &ctx()).await
}

async fn run_agent(graph: &Graph, state: State) -> Result<Outcome, RunFailure> {
    let (_sender, mut inbox) = channel();
    run(graph, &agent_config(graph), state, None, &ctx(), &mut inbox).await
}

async fn call_in(
    graph: &Graph,
    history: Conversation,
    ctx: &Context,
) -> Result<Outcome, RunFailure> {
    let state = graph
        .start_state([(key("history"), Value::conversation(history))])
        .unwrap();
    let (_sender, mut inbox) = channel();
    run(graph, &agent_config(graph), state, None, ctx, &mut inbox).await
}

fn finished(result: Result<Outcome, RunFailure>) -> State {
    match result.map_err(|f| f.error).unwrap() {
        Outcome::Finished { state, end } => {
            assert_eq!(end.as_str(), "done");
            state
        }
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

fn last_turn(state: &State) -> Turn {
    match state
        .conversation(&key("history"))
        .unwrap()
        .entries()
        .last()
    {
        Some(Entry::Turn(turn)) => turn.clone(),
        Some(Entry::UserInput(_)) | None => panic!("expected the agent's turn last"),
    }
}

#[test]
fn given_an_agent_when_built_then_its_signature_names_history_answer_and_the_limit_flag() {
    let model = Arc::new(LoggingModel::new(Vec::new()));
    let plain = agent(model.clone()).graph().unwrap();
    let signature = plain.signature();
    assert_eq!(
        signature.inputs,
        BTreeMap::from([(key("history"), Kind::Conversation)])
    );
    assert_eq!(
        signature.outputs,
        BTreeMap::from([
            (key("history"), Kind::Conversation),
            (key("answer"), Kind::Str),
        ])
    );
    assert_eq!(
        plain.schema().config,
        BTreeMap::from([(key("base"), Kind::Str)])
    );

    let flagged = ReactAgent {
        system: vec![Source::State(key("persona"))],
        thinking: Some(Switch::Config(key("deep"))),
        tool_concurrency: Some(Limit::Config(key("width"))),
        ..limited(
            model,
            2,
            OnLimit::End {
                node: nid("limit"),
                flag: key("limit_reached"),
            },
        )
    }
    .graph()
    .unwrap();
    assert_eq!(
        flagged.signature().inputs,
        BTreeMap::from([
            (key("history"), Kind::Conversation),
            (key("persona"), Kind::Str),
        ])
    );
    assert_eq!(
        flagged.signature().outputs.get(&key("limit_reached")),
        Some(&Kind::Bool)
    );
    assert_eq!(
        flagged.schema().config,
        BTreeMap::from([(key("deep"), Kind::Bool), (key("width"), Kind::Int)])
    );
}

#[test]
fn given_a_limit_flag_that_names_the_answer_when_built_then_flag_key_mismatch() {
    let model = Arc::new(LoggingModel::new(Vec::new()));
    let built = limited(
        model,
        1,
        OnLimit::Continue {
            node: nid("limit"),
            flag: key("answer"),
        },
    )
    .graph();
    assert!(matches!(built, Err(GraphError::FlagKeyMismatch { .. })));
}

#[test]
fn given_a_limit_flag_that_names_another_key_of_the_graph_when_built_then_key_collision() {
    let model = Arc::new(LoggingModel::new(Vec::new()));
    let on_limit = || OnLimit::End {
        node: nid("limit"),
        flag: key("topic"),
    };
    let source = ReactAgent {
        system: vec![Source::State(key("topic"))],
        ..limited(model.clone(), 1, on_limit())
    };
    let input = ReactAgent {
        inputs: vec![(key("topic"), Kind::Bool)],
        ..limited(model, 1, on_limit())
    };
    for agent in [source, input] {
        assert!(matches!(
            agent.graph(),
            Err(GraphError::KeyCollision { ref key }) if *key == self::key("topic")
        ));
    }
}

#[test]
fn given_a_tool_node_or_a_limit_node_named_like_a_node_of_the_agent_when_built_then_duplicate_node()
{
    let model = Arc::new(LoggingModel::new(Vec::new()));
    for reserved in ["llm", "answer"] {
        let split = ReactAgent {
            tool_nodes: vec![(nid(reserved), vec![Arc::new(EchoTool) as _])],
            ..agent(model.clone())
        };
        let limit = limited(
            model.clone(),
            1,
            OnLimit::End {
                node: nid(reserved),
                flag: key("limited"),
            },
        );
        for agent in [split, limit] {
            assert!(matches!(
                agent.graph(),
                Err(GraphError::DuplicateNode { ref id }) if *id == nid(reserved)
            ));
        }
    }
    let limit_on_tools = limited(
        model,
        1,
        OnLimit::End {
            node: nid("tools"),
            flag: key("limited"),
        },
    );
    assert!(matches!(
        limit_on_tools.graph(),
        Err(GraphError::DuplicateNode { ref id }) if *id == nid("tools")
    ));
}

#[tokio::test]
async fn given_a_plain_answer_when_the_agent_runs_then_history_and_answer_come_back() {
    let model = Arc::new(LoggingModel::new(vec![text_step("hello there")]));
    let graph = agent(model.clone()).graph().unwrap();
    let state = finished(call(&graph, question("hi")).await);
    assert_eq!(state.str(&key("answer")).unwrap(), "hello there");
    assert_eq!(
        state.conversation(&key("history")).unwrap().entries().len(),
        2
    );
    let requests = model.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].system.as_deref(),
        Some("You are a helpful agent.")
    );
    assert_eq!(requests[0].tools.len(), 1);
}

#[tokio::test]
async fn given_one_tool_round_when_the_agent_runs_then_the_turn_holds_the_round_and_the_reply() {
    let model = Arc::new(LoggingModel::new(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("echo said echoed"),
    ]));
    let graph = agent(model).graph().unwrap();
    let state = finished(call(&graph, question("hi")).await);
    assert_eq!(state.str(&key("answer")).unwrap(), "echo said echoed");
    let turn = last_turn(&state);
    assert_eq!(turn.items().len(), 3);
    assert!(matches!(turn.items()[1], TurnItem::ToolResults(_)));
}

#[tokio::test]
async fn given_a_final_step_of_several_text_blocks_when_the_agent_runs_then_answer_joins_them_verbatim()
 {
    let several = br_llm_messages::Step::new(
        vec![
            br_llm_messages::AssistantBlock::Text {
                text: Text::new("one ").unwrap(),
            },
            br_llm_messages::AssistantBlock::Text {
                text: Text::new("two").unwrap(),
            },
        ],
        br_llm_messages::StopReason::EndTurn,
        None,
        None,
    )
    .unwrap();
    let model = Arc::new(LoggingModel::new(vec![
        tool_call_step("c1", "echo", json!({})),
        several,
    ]));
    let graph = agent(model).graph().unwrap();
    let state = finished(call(&graph, question("hi")).await);
    assert_eq!(state.str(&key("answer")).unwrap(), "one two");
}

#[tokio::test]
async fn given_a_structured_output_when_the_agent_runs_then_answer_is_the_json_text_of_the_block() {
    let schema = json!({ "type": "object", "properties": { "n": { "type": "integer" } } });
    let model = Arc::new(LoggingModel::new(vec![
        tool_call_step("c1", "echo", json!({})),
        structured_step(json!({ "n": 3 })),
    ]));
    let graph = ReactAgent {
        output: OutputMode::Structured {
            schema: schema.clone(),
        },
        ..agent(model.clone())
    }
    .graph()
    .unwrap();
    let state = finished(call(&graph, question("count")).await);
    assert_eq!(state.str(&key("answer")).unwrap(), r#"{"n":3}"#);
    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| request.output
        == OutputMode::Structured {
            schema: schema.clone()
        }));
}

#[tokio::test]
async fn given_the_round_limit_ends_the_turn_when_the_agent_runs_then_limit_reached_is_true() {
    let model = Arc::new(LoggingModel::new(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
    ]));
    let graph = limited(
        model.clone(),
        1,
        OnLimit::End {
            node: nid("limit"),
            flag: key("limit_reached"),
        },
    )
    .graph()
    .unwrap();
    let state = finished(call(&graph, question("hi")).await);
    assert!(state.bool(&key("limit_reached")).unwrap());
    assert!(
        state
            .str(&key("answer"))
            .unwrap()
            .starts_with("Tool round limit reached (1)")
    );
    assert_eq!(model.requests().len(), 2);
}

#[tokio::test]
async fn given_the_round_limit_errors_when_the_agent_keeps_calling_then_the_run_fails() {
    let model = Arc::new(LoggingModel::new(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
    ]));
    let graph = limited(model, 1, OnLimit::Error).graph().unwrap();
    let error = call(&graph, question("hi")).await.err().unwrap().error;
    assert!(matches!(
        error,
        GraphError::ToolLimitReached { max_rounds: 1, .. }
    ));
}

#[tokio::test]
async fn given_a_history_with_a_finished_tool_round_when_called_again_then_the_turn_resumes() {
    let mut history = question("hi");
    let mut turn = Turn::new(
        TurnId::new("t0").unwrap(),
        Some(author()),
        tool_call_step("c1", "echo", json!({})),
    );
    turn.push_result(ToolResult::new(
        ToolCallId::new("c1").unwrap(),
        ToolName::new("echo").unwrap(),
        vec![ToolResultBlock::text(Text::new("echoed").unwrap())],
        false,
    ))
    .unwrap();
    history.push_turn(turn).unwrap();

    let model = Arc::new(LoggingModel::new(vec![text_step("resumed reply")]));
    let graph = agent(model.clone()).graph().unwrap();
    let state = finished(call(&graph, history).await);
    assert_eq!(state.str(&key("answer")).unwrap(), "resumed reply");
    assert_eq!(
        state.conversation(&key("history")).unwrap().entries().len(),
        2
    );
    let turn = last_turn(&state);
    assert_eq!(turn.id().as_str(), "t0");
    assert_eq!(turn.items().len(), 3);
    assert_eq!(model.requests().len(), 1);
}

#[tokio::test]
async fn given_a_new_activation_when_the_round_limit_counts_then_it_starts_from_the_open_turn() {
    let model = Arc::new(LoggingModel::new(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("first"),
        tool_call_step("c2", "echo", json!({})),
        text_step("second"),
    ]));
    let graph = limited(model, 1, OnLimit::Error).graph().unwrap();
    let ctx = ctx();
    let state = finished(call_in(&graph, question("hi"), &ctx).await);
    let mut history = state.conversation(&key("history")).unwrap().clone();
    history.push_input(
        UserInput::new(
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new("again").unwrap())],
        )
        .unwrap(),
    );
    let state = finished(call_in(&graph, history, &ctx).await);
    assert_eq!(state.str(&key("answer")).unwrap(), "second");
}

fn selective(model: Arc<LoggingModel>, writer: Arc<WriteFileTool>) -> ReactAgent {
    ReactAgent {
        tools: vec![Arc::new(EchoTool), writer],
        enabled: Some(key("offered")),
        ..agent(model)
    }
}

#[tokio::test]
async fn given_an_enabled_subset_when_the_agent_runs_then_only_it_is_offered_and_another_call_is_refused()
 {
    let model = Arc::new(LoggingModel::new(vec![
        calls_step(&["echo", "write_file"]),
        text_step("could not write"),
    ]));
    let writer = Arc::new(WriteFileTool::default());
    let graph = selective(model.clone(), writer.clone()).graph().unwrap();
    assert_eq!(
        graph.signature().inputs.get(&key("offered")),
        Some(&Kind::list(Kind::Str))
    );
    let state = graph
        .start_state([
            (key("history"), Value::conversation(question("hi"))),
            (key("offered"), Value::list(vec![Value::str("echo")])),
        ])
        .unwrap();
    let state = finished(run_agent(&graph, state).await);
    assert_eq!(state.str(&key("answer")).unwrap(), "could not write");
    assert_eq!(
        results_of(&last_turn(&state)),
        [
            ("echo".to_owned(), false, "echoed".to_owned()),
            (
                "write_file".to_owned(),
                true,
                "Tool write_file is not enabled for this run: this call was not executed."
                    .to_owned()
            ),
        ]
    );
    assert_eq!(writer.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    for request in requests {
        let offered: Vec<&str> = request
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect();
        assert_eq!(offered, ["echo"]);
    }
}

#[test]
fn given_an_enabled_key_that_names_a_key_of_the_graph_when_built_then_key_collision() {
    for name in ["history", "answer"] {
        let built = ReactAgent {
            enabled: Some(key(name)),
            ..agent(Arc::new(LoggingModel::new(Vec::new())))
        }
        .graph();
        assert!(
            matches!(built, Err(GraphError::KeyCollision { key: ref found }) if *found == key(name))
        );
    }
}

fn caller_schema() -> Schema {
    Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .state(key("answer"), Kind::Str)
        .state(key("questions"), Kind::list(Kind::Conversation))
        .state(key("question"), Kind::Conversation)
        .state(key("replies"), Kind::list(Kind::Str))
        .config(key("prompt"), Kind::Str)
        .config(key("deep"), Kind::Bool)
        .build()
}

fn caller_config() -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("prompt"), Value::str("Be brief."));
    values.insert(key("deep"), Value::bool(true));
    Config::new(&caller_schema(), values).unwrap()
}

fn thinking_agent(model: Arc<LoggingModel>) -> Arc<Graph> {
    ReactAgent {
        thinking: Some(Switch::Config(key("deep"))),
        ..agent(model)
    }
    .graph()
    .unwrap()
}

async fn run_caller(graph: &Graph, state: State) -> State {
    let (_sender, mut inbox) = channel();
    match run(graph, &caller_config(), state, None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap()
    {
        Outcome::Finished { state, .. } => state,
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

#[tokio::test]
async fn given_the_agent_called_as_a_subgraph_when_run_then_the_caller_gets_history_and_answer() {
    let model = Arc::new(LoggingModel::new(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("done with echo"),
    ]));
    let call = SubGraph::call(thinking_agent(model.clone()))
        .input(key("history"), Input::From(key("chat")))
        .config(key("base"), Input::Config(key("prompt")))
        .config(key("deep"), Input::Config(key("deep")))
        .output(key("history"), Output::Set(key("chat")))
        .output(key("answer"), Output::Set(key("answer")));
    let graph = GraphBuilder::new(caller_schema())
        .entry(nid("agent"))
        .subgraph(nid("agent"), call)
        .edge(
            nid("agent"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .input(key("chat"))
        .build()
        .unwrap();
    let state = graph
        .start_state([(key("chat"), Value::conversation(question("hi")))])
        .unwrap();
    let state = run_caller(&graph, state).await;
    assert_eq!(state.str(&key("answer")).unwrap(), "done with echo");
    assert_eq!(state.conversation(&key("chat")).unwrap().entries().len(), 2);
    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|request| request.thinking == Some(true))
    );
    assert_eq!(requests[0].system.as_deref(), Some("Be brief."));
}

#[tokio::test]
async fn given_a_pass_through_input_mapped_by_the_caller_when_a_tool_runs_then_it_reads_the_value()
{
    let model = Arc::new(LoggingModel::new(vec![
        tool_call_step("c1", "scope", json!({})),
        text_step("done"),
    ]));
    let agent = ReactAgent {
        tools: vec![Arc::new(ScopeTool)],
        inputs: vec![(key("scope"), Kind::Str)],
        ..agent(model)
    }
    .graph()
    .unwrap();
    assert_eq!(
        agent.signature().inputs,
        BTreeMap::from([
            (key("history"), Kind::Conversation),
            (key("scope"), Kind::Str),
        ])
    );
    let call = SubGraph::call(agent)
        .input(key("history"), Input::From(key("chat")))
        .input(key("scope"), Input::From(key("folder")))
        .config(key("base"), Input::Config(key("prompt")))
        .output(key("history"), Output::Set(key("chat")));
    let schema = Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .state(key("folder"), Kind::Str)
        .config(key("prompt"), Kind::Str)
        .config(key("deep"), Kind::Bool)
        .build();
    let graph = GraphBuilder::new(schema)
        .entry(nid("agent"))
        .subgraph(nid("agent"), call)
        .edge(
            nid("agent"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .input(key("chat"))
        .input(key("folder"))
        .build()
        .unwrap();
    let state = graph
        .start_state([
            (key("chat"), Value::conversation(question("hi"))),
            (key("folder"), Value::str("notes/2026")),
        ])
        .unwrap();
    let state = run_caller(&graph, state).await;
    let Some(Entry::Turn(turn)) = state.conversation(&key("chat")).unwrap().entries().last() else {
        panic!("expected the agent's turn");
    };
    assert_eq!(
        results_of(turn),
        [("scope".to_owned(), false, "scope is notes/2026".to_owned())]
    );
}

#[test]
fn given_a_pass_through_input_that_names_a_key_of_the_graph_when_built_then_key_collision() {
    let flagged = |inputs: Vec<(crate::value::Key, Kind)>| {
        ReactAgent {
            system: vec![Source::State(key("persona"))],
            enabled: Some(key("offered")),
            inputs,
            ..limited(
                Arc::new(LoggingModel::new(Vec::new())),
                1,
                OnLimit::End {
                    node: nid("limit"),
                    flag: key("limit_reached"),
                },
            )
        }
        .graph()
    };
    for name in ["history", "answer", "persona", "offered", "limit_reached"] {
        let built = flagged(vec![(key(name), Kind::Str)]);
        assert!(
            matches!(built, Err(GraphError::KeyCollision { key: ref found }) if *found == key(name)),
            "{name}"
        );
    }
    let built = flagged(vec![(key("scope"), Kind::Str), (key("scope"), Kind::Int)]);
    assert!(matches!(built, Err(GraphError::KeyCollision { .. })));
    assert!(flagged(vec![(key("scope"), Kind::Str)]).is_ok());
}

#[test]
fn given_a_key_read_by_several_sources_when_built_then_it_is_declared_once() {
    let graph = ReactAgent {
        system: vec![
            Source::State(key("persona")),
            Source::Config(key("base")),
            Source::State(key("persona")),
            Source::Config(key("base")),
        ],
        ..agent(Arc::new(LoggingModel::new(Vec::new())))
    }
    .graph()
    .unwrap();
    assert_eq!(
        graph.signature().inputs,
        BTreeMap::from([
            (key("history"), Kind::Conversation),
            (key("persona"), Kind::Str),
        ])
    );
    assert_eq!(
        graph.schema().config,
        BTreeMap::from([(key("base"), Kind::Str)])
    );
}

#[test]
fn given_a_source_that_names_a_key_of_the_graph_when_built_then_key_collision() {
    let flagged = |source: Source| {
        ReactAgent {
            system: vec![source],
            thinking: Some(Switch::Config(key("deep"))),
            ..limited(
                Arc::new(LoggingModel::new(Vec::new())),
                1,
                OnLimit::End {
                    node: nid("limit"),
                    flag: key("limit_reached"),
                },
            )
        }
        .graph()
    };
    for name in ["history", "answer", "limit_reached"] {
        let built = flagged(Source::State(key(name)));
        assert!(
            matches!(built, Err(GraphError::KeyCollision { key: ref found }) if *found == key(name)),
            "{name}"
        );
    }
    let built = flagged(Source::Config(key("deep")));
    assert!(
        matches!(built, Err(GraphError::KeyCollision { key: ref found }) if *found == key("deep"))
    );
}

#[tokio::test]
async fn given_the_agent_as_a_map_body_when_run_then_each_question_gets_its_reply_in_order() {
    let model = Arc::new(LoggingModel::new(vec![
        text_step("one"),
        tool_call_step("c1", "echo", json!({})),
        text_step("two"),
    ]));
    let body = SubGraph::call(thinking_agent(model))
        .input(key("history"), Input::From(key("question")))
        .config(key("base"), Input::Config(key("prompt")))
        .config(key("deep"), Input::Config(key("deep")))
        .output(key("answer"), Output::Append(key("replies")));
    let map = Map {
        list: key("questions"),
        item: key("question"),
        body: Box::new(body),
        max_concurrency: Some(Limit::Fixed(NonZeroUsize::new(1).unwrap())),
        on_item_failure: ItemFailure::Finish,
    };
    let graph = GraphBuilder::new(caller_schema())
        .entry(nid("each"))
        .map(nid("each"), map)
        .edge(
            nid("each"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .input(key("questions"))
        .build()
        .unwrap();
    let questions = Value::list(vec![
        Value::conversation(question("first?")),
        Value::conversation(question("second?")),
    ]);
    let state = graph.start_state([(key("questions"), questions)]).unwrap();
    let state = run_caller(&graph, state).await;
    assert_eq!(
        state.list(&key("replies")).unwrap(),
        &[Value::str("one"), Value::str("two")]
    );
}
