//! The requests an agent asks its turn ids under, and what the id source
//! gives back for them.

use std::sync::Arc;

use br_llm_messages::InputId;

use crate::graph::{Always, Context, GraphBuilder, IdRequest, Input, SubGraph, Target};
use crate::observe::NoopObserver;
use crate::origin::{OccurrenceKey, Origin, RunId};
use crate::react::react_agent_support::*;
use crate::react::test_support::*;
use crate::run::{control, run};
use crate::state::{Kind, Schema, Value};
use crate::testkit::SeqIds;
use crate::value::EndLabel;

fn ctx_with(ids: &Arc<SeqIds>, run: &str) -> Context {
    Context::new(Arc::new(NoopObserver), ids.clone()).with_run_id(RunId::new(run))
}

fn request(run: &str, occurrence: &str, trigger: &str, rank: usize) -> IdRequest {
    IdRequest::new(
        Origin {
            run: RunId::new(run),
            occurrence: OccurrenceKey::parse(occurrence).unwrap(),
        },
        Some(InputId::new(trigger).unwrap()),
        rank,
    )
}

/// The id of the agent's last turn after a call answering `body`.
async fn answer_turn_id(ids: &Arc<SeqIds>, run: &str, body: &str) -> String {
    let client = mock(vec![text_step("ok")]);
    let graph = agent(&client).graph().unwrap();
    let state = finished(call_in(&graph, question(body), &ctx_with(ids, run)).await);
    last_turn(&state).id().as_str().to_owned()
}

#[tokio::test]
async fn given_an_agent_at_top_level_when_it_answers_then_its_turn_id_is_asked_under_its_origin_trigger_and_rank_one()
 {
    let ids = Arc::new(SeqIds::new());
    let id = answer_turn_id(&ids, "r1", "hi").await;
    assert_eq!(id, "t1");
    assert_eq!(ids.requests(), vec![request("r1", "", "i-hi", 1)]);
}

#[tokio::test]
async fn given_the_same_run_and_trigger_when_run_again_then_the_same_turn_id() {
    let ids = Arc::new(SeqIds::new());
    let first = answer_turn_id(&ids, "r1", "hi").await;
    let again = answer_turn_id(&ids, "r1", "hi").await;
    assert_eq!(first, again);
    assert_eq!(ids.requests().len(), 1);
}

#[tokio::test]
async fn given_a_new_run_or_a_new_trigger_when_run_then_a_new_turn_id() {
    let ids = Arc::new(SeqIds::new());
    let first = answer_turn_id(&ids, "r1", "hi").await;
    let new_run = answer_turn_id(&ids, "r2", "hi").await;
    let new_trigger = answer_turn_id(&ids, "r1", "again").await;
    assert_ne!(first, new_run);
    assert_ne!(first, new_trigger);
    assert_ne!(new_run, new_trigger);
}

#[tokio::test]
async fn given_an_agent_called_by_a_node_when_it_answers_then_its_ids_are_asked_under_the_calling_node()
 {
    let client = mock(vec![text_step("ok")]);
    let schema = Schema::builder()
        .state_by_name(key("chat"), Kind::Conversation)
        .config_by_name(key("base"), Kind::Str)
        .build();
    let call = SubGraph::call(agent(&client).graph().unwrap())
        .input(key("history"), Input::From(key("chat")))
        .config(key("base"), Input::Config(key("base")));
    let graph = GraphBuilder::new(schema)
        .entry(nid("helper"))
        .subgraph(nid("helper"), call)
        .edge(
            nid("helper"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .input(key("chat"))
        .build()
        .unwrap();
    let state = graph
        .start_state([(key("chat"), Value::conversation(question("hi")))])
        .unwrap();
    let ids = Arc::new(SeqIds::new());
    let (_sender, mut channel) = control();
    run(
        &graph,
        &agent_config(&graph),
        state,
        None,
        &ctx_with(&ids, "r1"),
        &mut channel,
    )
    .await
    .map_err(|failure| failure.error)
    .unwrap();
    assert_eq!(ids.requests(), vec![request("r1", "helper", "i-hi", 1)]);
}

#[test]
fn given_a_nested_occurrence_when_its_parent_is_taken_then_the_last_segment_goes() {
    let key = OccurrenceKey::parse("a/m[2]/c").unwrap();
    assert_eq!(key.parent(), OccurrenceKey::parse("a/m[2]").unwrap());
    assert_eq!(OccurrenceKey::root().parent(), OccurrenceKey::root());
}
