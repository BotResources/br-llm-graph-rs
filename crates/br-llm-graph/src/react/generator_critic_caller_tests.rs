//! The generator-critic called by another graph: as a call whose outputs
//! the caller reads, and as the body of a map.

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_messages::Conversation;

use crate::graph::{
    Always, Graph, GraphBuilder, Input, ItemFailure, Map, Output, SubGraph, Target,
};
use crate::react::generator_critic_support::*;
use crate::react::test_support::*;
use crate::run::{Outcome, control, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::value::EndLabel;

fn caller_schema() -> Schema {
    Schema::builder()
        .state_by_name(key("chat"), Kind::Conversation)
        .state_by_name(key("best"), Kind::Str)
        .state_by_name(key("count"), Kind::Int)
        .state_by_name(key("label"), Kind::Str)
        .state_by_name(key("questions"), Kind::list(Kind::Conversation))
        .state_by_name(key("question"), Kind::Conversation)
        .state_by_name(key("answers"), Kind::list(Kind::Str))
        .state_by_name(key("verdicts"), Kind::list(Kind::Bool))
        .build()
}

fn duo_call(graph: Arc<Graph>, conversation: &str) -> SubGraph {
    SubGraph::call(graph)
        .input(key("conversation"), Input::From(key(conversation)))
        .input(
            key("generator_history"),
            Input::Const(Value::conversation(Conversation::new())),
        )
}

async fn run_caller(graph: &Graph, state: State) -> State {
    let config = Config::new(graph.schema(), BTreeMap::new()).unwrap();
    let (_sender, mut channel) = control();
    match run(graph, &config, state, None, &ctx(), &mut channel)
        .await
        .map_err(|f| f.error)
        .unwrap()
    {
        Outcome::Finished { state, .. } => state,
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

#[tokio::test]
async fn given_the_duo_called_as_a_subgraph_when_run_then_the_caller_reads_the_answer_and_the_label()
 {
    let generator_client = mock(vec![text_step("a"), text_step("b")]);
    let critic_client = mock(vec![verdict(false, "fix it"), verdict(true, "")]);
    let call = duo_call(
        duo(&generator_client, &critic_client).graph().unwrap(),
        "chat",
    )
    .output(key("answer"), Output::Set(key("best")))
    .output(key("nb_generations"), Output::Set(key("count")))
    .output_end_label(Output::Set(key("label")));
    let graph = GraphBuilder::new(caller_schema())
        .entry(nid("duo"))
        .subgraph(nid("duo"), call)
        .edge(
            nid("duo"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .input(key("chat"))
        .build()
        .unwrap();
    let state = graph
        .start_state([(key("chat"), Value::conversation(question("explain")))])
        .unwrap();
    let state = run_caller(&graph, state).await;
    assert_eq!(state.str(&key("label")).unwrap(), "validated");
    assert_eq!(state.str(&key("best")).unwrap(), "b");
    assert_eq!(state.int(&key("count")).unwrap(), 2);
}

#[tokio::test]
async fn given_the_duo_as_a_map_body_when_run_then_each_question_gets_its_answer_in_order() {
    let generator_client = mock(vec![
        text_step("one"),
        text_step("two"),
        text_step("two, revised"),
    ]);
    let critic_client = mock(vec![
        verdict(true, ""),
        verdict(false, "revise"),
        verdict(true, ""),
    ]);
    let body = duo_call(
        duo(&generator_client, &critic_client).graph().unwrap(),
        "question",
    )
    .output(key("answer"), Output::Append(key("answers")))
    .output(key("is_validated"), Output::Append(key("verdicts")));
    let map = Map {
        list: key("questions"),
        item: key("question"),
        body: Box::new(body),
        max_concurrency: Some(limit(1)),
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
        state.list(&key("answers")).unwrap(),
        &[Value::str("one"), Value::str("two, revised")]
    );
    assert_eq!(
        state.list(&key("verdicts")).unwrap(),
        &[Value::bool(true), Value::bool(true)]
    );
}
