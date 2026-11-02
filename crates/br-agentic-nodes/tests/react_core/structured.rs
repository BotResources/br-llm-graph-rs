use std::sync::Arc;

use br_agentic_nodes::{FallbackList, ReactAgent};
use br_llm_messages::{ClosureReason, ResponseEnding};

use crate::common::*;
use crate::kit::*;

fn shaped(rig: &Rig, shapes: &Arc<Shapes>, fallback: FallbackList) -> ReactAgent<Doc> {
    ReactAgent::new(
        "agent",
        rig.ports().with_output_schema(shapes.clone()),
        rig.settings.clone(),
        rig.react_spec(fallback),
    )
}

fn giving() -> Arc<Shapes> {
    Arc::new(Shapes::giving(reply_shape()))
}

#[tokio::test(start_paused = true)]
async fn given_a_shape_when_the_agent_steps_then_every_step_asks_for_a_structured_reply_in_that_shape_tools_included()
 {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]);
    let model = Model::new(vec![
        Act::call("alpha"),
        Act::structured(r#"{"answer":"yes"}"#),
    ]);
    let node = shaped(&rig, &giving(), plains(&[&model]));

    Driver::result(&node).await;

    let requests = model.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(asked_shape(request), Some(&reply_shape()));
        assert!(!request.tools().is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_port_that_gives_no_shape_when_the_agent_steps_then_the_step_asks_for_plain_text() {
    let rig = Rig::new();
    let model = Model::new(vec![Act::text("plain")]);
    let node = shaped(&rig, &Arc::new(Shapes::none()), plains(&[&model]));

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "plain");
    assert_eq!(asked_shape(&model.requests()[0]), None);
}

#[tokio::test(start_paused = true)]
async fn given_a_shape_when_the_instance_starts_then_the_port_is_called_once_for_the_label_agent() {
    let rig = Rig::new();
    let model = Model::new(vec![Act::structured(r#"{"answer":"yes"}"#)]);
    let shapes = giving();
    let node = shaped(&rig, &shapes, plains(&[&model]));

    Driver::result(&node).await;

    assert_eq!(shapes.labels(), vec!["agent"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_shape_when_the_final_response_holds_a_structured_block_then_the_outcome_gives_it()
{
    let rig = Rig::new();
    let model = Model::new(vec![Act::structured(r#"{"answer":"yes"}"#)]);
    let capture = Capture::new(&rig.record);
    let mut spec = rig.react_spec(plains(&[&model]));
    spec.outcome = capture.clone();
    let node = ReactAgent::new(
        "agent",
        rig.ports().with_output_schema(giving()),
        rig.settings.clone(),
        spec,
    );

    Driver::result(&node).await;

    let outcome = capture.only();
    let reply = outcome.structured_reply().expect("a structured reply");
    assert_eq!(
        reply.value(),
        &br_llm_messages::OpaqueJson::parse(r#"{"answer":"yes"}"#).unwrap()
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_shape_and_two_clients_when_the_first_final_response_holds_no_structured_block_then_the_next_client_answers_and_the_open_response_is_discarded()
 {
    let rig = Rig::new();
    let first = Model::new(vec![Act::text("plain")]);
    let second = Model::new(vec![Act::structured(r#"{"answer":"yes"}"#)]);
    let node = shaped(&rig, &giving(), plains(&[&first, &second]));

    Driver::result(&node).await;

    assert_eq!(
        discarded_responses(&rig.record),
        vec![first.requests()[0].reply_entry()]
    );
    assert_eq!(
        response_ids(&rig.conversation()),
        vec![second.requests()[0].reply_entry()]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_shape_and_one_client_when_the_first_final_response_holds_no_structured_block_then_the_instance_fails()
 {
    let rig = Rig::new();
    let only = Model::new(vec![Act::text("plain")]);
    let node = shaped(&rig, &giving(), plains(&[&only]));

    let failure = Driver::failure(&node).await;

    assert!(failure.contains("structured block"), "{failure}");
    assert_eq!(only.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_shape_when_a_step_after_a_tool_run_ends_with_no_structured_block_then_it_is_a_fault_and_the_next_client_is_not_tried()
 {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]);
    let first = Model::new(vec![Act::call("alpha"), Act::text("plain")]);
    let second = Model::new(vec![Act::structured(r#"{"answer":"yes"}"#)]);
    let node = shaped(&rig, &giving(), plains(&[&first, &second]));

    let failure = Driver::failure(&node).await;

    assert!(failure.contains("structured block"), "{failure}");
    assert_eq!((first.calls(), second.calls()), (2, 0));
    let turns = ai_turns(&rig.conversation()).len();
    assert_eq!(turns, 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_shape_when_responses_ask_for_more_then_they_are_not_checked() {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]);
    let model = Model::new(vec![
        Act::call("alpha"),
        Act::continuation(),
        Act::pending_execution(),
        Act::structured(r#"{"answer":"yes"}"#),
    ]);
    let capture = Capture::new(&rig.record);
    let mut spec = rig.react_spec(plains(&[&model]));
    spec.outcome = capture.clone();
    let node = ReactAgent::new(
        "agent",
        rig.ports().with_output_schema(giving()),
        rig.settings.clone(),
        spec,
    );

    Driver::result(&node).await;

    assert_eq!(model.calls(), 4);
    assert!(capture.only().structured_reply().is_some());
}

#[tokio::test(start_paused = true)]
async fn given_a_shape_when_the_final_response_ends_with_a_refusal_and_no_structured_block_then_it_is_exempt()
 {
    for ending in [
        ResponseEnding::refusal(),
        ResponseEnding::filtering(),
        ResponseEnding::output_exhaustion(),
    ] {
        let rig = Rig::new();
        let model = Model::new(vec![Act::ended("no", ending.clone())]);
        let capture = Capture::new(&rig.record);
        let mut spec = rig.react_spec(plains(&[&model]));
        spec.outcome = capture.clone();
        let node = ReactAgent::new(
            "agent",
            rig.ports().with_output_schema(giving()),
            rig.settings.clone(),
            spec,
        );

        Driver::result(&node).await;

        let outcome = capture.only();
        assert_eq!(outcome.response().ending(), &ending);
        assert!(outcome.structured_reply().is_none());
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_shape_when_the_step_limit_stops_the_work_then_the_last_response_is_not_checked() {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]).with_step_limit(1);
    let model = Model::new(vec![Act::call("alpha")]);
    let node = shaped(&rig, &giving(), plains(&[&model]));

    let update = Driver::result(&node).await;

    assert!(update.step_limit);
}

#[tokio::test(start_paused = true)]
async fn given_a_failing_output_schema_port_when_the_instance_starts_then_it_fails_before_any_step()
{
    let rig = Rig::new();
    let model = Model::new(vec![Act::structured(r#"{"answer":"yes"}"#)]);
    let node = shaped(&rig, &Arc::new(Shapes::failing()), plains(&[&model]));

    let failure = Driver::failure(&node).await;

    assert!(failure.contains("output-schema port failed"), "{failure}");
    assert_eq!(model.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_shape_when_a_restored_final_response_holds_no_structured_block_then_it_is_taken_as_it_is()
 {
    let rig = seeded(|rig, conversation| {
        seed_turn(
            conversation,
            rig.agent,
            text_part("restored"),
            ResponseEnding::finished(),
            Some(ClosureReason::completed()),
        );
    });
    let model = Model::new(vec![]);
    let capture = Capture::new(&rig.record);
    let mut spec = rig.react_spec(plains(&[&model]));
    spec.outcome = capture.clone();
    let node = ReactAgent::new(
        "agent",
        rig.ports().with_output_schema(giving()),
        rig.settings.clone(),
        spec,
    );

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "restored");
    assert_eq!(model.calls(), 0);
    assert!(capture.only().structured_reply().is_none());
}
