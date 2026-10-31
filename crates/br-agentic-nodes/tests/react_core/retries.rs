use std::time::Duration;

use br_llm_clients_core::ClientError;

use crate::common::*;
use crate::kit::*;

#[tokio::test(start_paused = true)]
async fn given_a_tool_that_faults_when_the_instance_fails_then_the_tool_was_started_once() {
    let record_rig = Rig::new();
    let (boom, spy_boom) = spy("boom", Plan::Fault, &record_rig.record);
    let rig = record_rig.with_tools(vec![boom]);
    let model = Model::new(vec![Act::call("boom"), Act::call("boom")]);
    let node = node(&rig, plains(&[&model]));

    Driver::failure(&node).await;

    assert_eq!(spy_boom.starts(), 1);
    assert_eq!(model.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_that_times_out_when_the_run_goes_on_then_the_tool_was_started_once() {
    let record_rig = Rig::new().with_timeouts(Duration::from_secs(5), Duration::from_secs(2));
    let (stuck, spy_stuck) = spy("stuck", Plan::Hang, &record_rig.record);
    let rig = record_rig.with_tools(vec![stuck]);
    let model = Model::new(vec![Act::call("stuck"), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    assert_eq!(spy_stuck.starts(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_an_outcome_port_that_fails_when_the_instance_ends_then_the_port_was_called_once() {
    let rig = Rig::new();
    let model = Model::new(vec![Act::text("done")]);
    let capture = Capture::failing(&rig.record);
    let node = node_capturing(&rig, plains(&[&model]), &capture);

    Driver::failure(&node).await;

    assert_eq!(capture.attempts(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_hand_over_that_fails_when_the_response_is_completed_then_it_is_not_retried_and_the_instance_fails()
 {
    let rig = Rig::new();
    let model = Model::new(vec![Act::text("done")]);
    let quirks = Quirks {
        fail_complete_response: true,
        ..Quirks::default()
    };
    let access = Wrapped::new(&rig.access, quirks);
    let node = node_over(&rig, access.clone(), plains(&[&model]));

    Driver::failure(&node).await;

    assert_eq!(access.attempts("complete_response"), 1);
    assert_eq!(model.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_hand_over_that_fails_when_the_response_is_opened_then_no_other_client_is_tried() {
    let rig = Rig::new();
    let first = Model::new(vec![Act::text("one")]);
    let second = Model::new(vec![Act::text("two")]);
    let quirks = Quirks {
        fail_open_response: true,
        ..Quirks::default()
    };
    let access = Wrapped::new(&rig.access, quirks);
    let node = node_over(&rig, access.clone(), plains(&[&first, &second]));

    Driver::failure(&node).await;

    assert_eq!(access.attempts("open_response"), 1);
    assert_eq!((first.calls(), second.calls()), (0, 0));
}

#[tokio::test(start_paused = true)]
async fn given_a_hand_over_that_fails_when_a_result_is_completed_then_it_is_not_retried_and_the_tool_ran_once()
 {
    let record_rig = Rig::new();
    let (alpha, spy_alpha) = spy(
        "alpha",
        Plan::Text("alpha done".to_owned()),
        &record_rig.record,
    );
    let rig = record_rig.with_tools(vec![alpha]);
    let model = Model::new(vec![Act::call("alpha"), Act::text("done")]);
    let quirks = Quirks {
        fail_complete_result: true,
        ..Quirks::default()
    };
    let access = Wrapped::new(&rig.access, quirks);
    let node = node_over(&rig, access.clone(), plains(&[&model]));

    Driver::failure(&node).await;

    assert_eq!(access.attempts("complete_result"), 1);
    assert_eq!(spy_alpha.starts(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_single_client_when_its_call_fails_then_the_call_is_not_retried_on_the_same_client()
{
    let rig = Rig::new();
    let only = Model::new(vec![
        Act::fail(ClientError::RateLimited { retry_after: None }),
        Act::text("never"),
    ]);
    let node = node(&rig, plains(&[&only]));

    Driver::failure(&node).await;

    assert_eq!(only.calls(), 1);
}
