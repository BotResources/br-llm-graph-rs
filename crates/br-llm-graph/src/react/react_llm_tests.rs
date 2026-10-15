//! The model-call step of the ReAct loop: what it sends, what it does with
//! the reply, and how a refused request or a failing client fails it.

use std::error::Error;
use std::sync::Arc;

use br_llm_clients_core::{
    BuildError, ClientError, OutputMode, Parameter, ParameterKind, Parameters, StructuredShape,
};
use br_llm_clients_mock::MockClient;
use br_llm_messages::StreamEvent;

use crate::error::{GraphError, NodeFault};
use crate::graph::{Always, Context, Graph, GraphBuilder, Target};
use crate::observe::{NoopObserver, Observer};
use crate::react::llm_node::LlmNode;
use crate::react::react_loop::ReactLoop;
use crate::react::source::Source;
use crate::react::test_support::*;
use crate::run::{Outcome, RunFailure, channel, run};
use crate::state::{State, Value};
use crate::testkit::SeqIds;
use crate::update::Update;
use crate::value::{EndLabel, NodeId};

pub(crate) fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

pub(crate) fn ctx() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

pub(crate) fn llm(client: &MockClient, system: Vec<Source>) -> LlmNode {
    LlmNode {
        key: key("chat"),
        author: author(),
        client: Arc::new(client.clone()),
        system,
        tools: Vec::new(),
        enabled: None,
        output: OutputMode::Plain,
    }
}

/// A graph of the one model-call step `llm`, ending `done`.
pub(crate) fn single(llm: LlmNode) -> Graph {
    GraphBuilder::new(schema())
        .entry(nid("llm"))
        .node(nid("llm"), llm)
        .edge(
            nid("llm"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap()
}

pub(crate) async fn run_on(
    graph: &Graph,
    state: State,
    ctx: &Context,
) -> Result<Outcome, RunFailure> {
    let (_sender, mut inbox) = channel();
    run(graph, &config(), state, None, ctx, &mut inbox).await
}

fn final_state(outcome: Outcome) -> State {
    match outcome {
        Outcome::Finished { state, .. } => state,
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected the run to finish"),
    }
}

#[tokio::test]
async fn given_a_reply_when_the_step_runs_then_it_is_a_new_turn_of_the_author() {
    let client = mock(vec![text_step("hello")]);
    let graph = single(llm(&client, vec![Source::Config(key("base"))]));

    let state = final_state(run_on(&graph, seeded_state(), &ctx()).await.unwrap());

    let chat = state.conversation(&key("chat")).unwrap();
    let turn = match chat.entries().last() {
        Some(br_llm_messages::Entry::Turn(turn)) => turn,
        other => panic!("expected a turn, got {other:?}"),
    };
    assert_eq!(turn.author(), Some(&author()));
    assert_eq!(turn.id().as_str(), "t1");
    let request = &requests(&client)[0];
    assert_eq!(
        request.messages(),
        crate::react::helpers::wire(
            seeded_state().conversation(&key("chat")).unwrap(),
            &author()
        )
        .unwrap()
        .as_slice()
    );
    assert_eq!(request.output(), &OutputMode::Plain);
    assert!(request.tools().is_empty());
}

#[tokio::test]
async fn given_a_structured_output_mode_when_the_step_runs_then_the_request_carries_it() {
    let shape = StructuredShape::new(
        "answer",
        None,
        Parameters::new(vec![
            Parameter::new("value", "", true, ParameterKind::String).unwrap(),
        ])
        .unwrap(),
    )
    .unwrap();
    let client = mock(vec![structured_step(serde_json::json!({ "value": "x" }))]);
    let mut node = llm(&client, Vec::new());
    node.output = OutputMode::Structured(shape.clone());

    run_on(&single(node), seeded_state(), &ctx()).await.unwrap();

    assert_eq!(
        requests(&client)[0].output(),
        &OutputMode::Structured(shape)
    );
}

#[tokio::test]
async fn given_enabled_filter_when_llm_runs_then_only_enabled_tools_declared() {
    let client = mock(vec![text_step("done")]);
    let mut node = llm(&client, vec![Source::Config(key("base"))]);
    node.tools = vec![Arc::new(EchoTool), Arc::new(TallyTool)];
    node.enabled = Some(key("enabled"));
    let mut state = seeded_state();
    state
        .apply_batch(&[Update::Set {
            key: key("enabled"),
            value: Value::list(vec![Value::str("echo"), Value::str("unknown")]),
        }])
        .unwrap();

    run_on(&single(node), state, &ctx()).await.unwrap();

    assert_eq!(tool_names(&requests(&client)[0]), vec!["echo"]);
}

#[tokio::test]
async fn given_no_enabled_key_when_llm_runs_then_every_tool_is_declared_in_order() {
    let client = mock(vec![text_step("done")]);
    let mut node = llm(&client, Vec::new());
    node.tools = vec![Arc::new(TallyTool), Arc::new(EchoTool)];

    run_on(&single(node), seeded_state(), &ctx()).await.unwrap();

    assert_eq!(tool_names(&requests(&client)[0]), vec!["tally", "echo"]);
}

#[tokio::test]
async fn given_failing_client_when_llm_runs_then_model_error_carrying_the_client_error() {
    let client = failing(ClientError::Unavailable { retry_after: None });
    let graph = single(llm(&client, vec![Source::Config(key("base"))]));

    let failure = run_on(&graph, seeded_state(), &ctx()).await.err().unwrap();

    let error = returned(&failure.error);
    assert!(matches!(
        error,
        GraphError::Model(ClientError::Unavailable { retry_after: None })
    ));
    let source = error.source().unwrap();
    assert_eq!(
        source.downcast_ref::<ClientError>(),
        Some(&ClientError::Unavailable { retry_after: None })
    );
    assert_eq!(
        failure
            .checkpoint
            .state
            .conversation(&key("chat"))
            .unwrap()
            .entries()
            .len(),
        1
    );
}

#[tokio::test]
async fn given_a_request_core_refuses_to_build_when_llm_runs_then_request_carrying_the_build_error()
{
    // Two tools of one name: br-llm-clients-core refuses the request. The
    // ReAct loop refuses such tools at build; the step alone does not.
    let client = mock(vec![text_step("unused")]);
    let mut node = llm(&client, Vec::new());
    node.tools = vec![Arc::new(EchoTool), Arc::new(EchoTool)];

    let failure = run_on(&single(node), seeded_state(), &ctx())
        .await
        .err()
        .unwrap();

    let error = returned(&failure.error);
    let duplicate = BuildError::DuplicateToolName {
        name: br_llm_messages::ToolName::new("echo").unwrap(),
    };
    assert!(matches!(error, GraphError::Request(built) if *built == duplicate));
    assert_eq!(
        error.source().unwrap().downcast_ref::<BuildError>(),
        Some(&duplicate)
    );
    assert_eq!(client.calls(), 0, "a refused request reaches no client");
}

#[tokio::test]
async fn given_failing_model_in_react_loop_when_run_then_node_failed_not_looping() {
    let client = failing(ClientError::Transport);
    let mut node = llm(&client, vec![Source::Config(key("base"))]);
    node.tools = vec![Arc::new(EchoTool)];
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![(nid("tools"), vec![Arc::new(EchoTool)])],
        after: Target::End(EndLabel::new("done").unwrap()),
        tool_concurrency: None,
        round_limit: None,
    };
    let graph = react
        .add(GraphBuilder::new(schema()).entry(nid("llm")), node)
        .unwrap()
        .build()
        .unwrap();

    let failure = run_on(&graph, seeded_state(), &ctx()).await.err().unwrap();

    assert!(matches!(
        failure.error,
        GraphError::NodeFailed {
            source: NodeFault::Returned(_),
            ..
        }
    ));
    assert_eq!(client.calls(), 1);
}

#[tokio::test]
async fn given_a_streamed_reply_when_llm_runs_then_the_observer_gets_each_event_with_the_conversation_key()
 {
    let client = mock(vec![text_step("hello")]);
    let recorder = Arc::new(StreamRecorder::default());
    let observer: Arc<dyn Observer> = recorder.clone();
    let ctx = Context::new(observer, Arc::new(SeqIds::new()));

    run_on(&single(llm(&client, Vec::new())), seeded_state(), &ctx)
        .await
        .unwrap();

    let events = recorder.events();
    let derived = br_llm_clients_mock::derive_events(
        &text_step("hello"),
        &br_llm_clients_mock::Chunking::none(),
    )
    .unwrap();
    assert_eq!(
        events
            .iter()
            .map(|(_, _, event)| event.clone())
            .collect::<Vec<_>>(),
        derived.events().to_vec()
    );
    assert!(
        events
            .iter()
            .all(|(occurrence, key, _)| occurrence == "llm" && key == "chat")
    );
    assert!(matches!(
        events.last(),
        Some((_, _, StreamEvent::Finish { .. }))
    ));
}
