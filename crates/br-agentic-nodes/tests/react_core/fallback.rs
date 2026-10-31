use br_llm_clients_core::ClientError;

use crate::common::*;
use crate::kit::*;

fn unavailable() -> ClientError {
    ClientError::Unavailable { retry_after: None }
}

#[tokio::test(start_paused = true)]
async fn given_one_client_when_its_call_fails_then_it_is_not_retried_and_the_instance_fails() {
    let rig = Rig::new();
    let only = Model::new(vec![Act::fail(unavailable()), Act::text("never")]);
    let node = node(&rig, plains(&[&only]));

    Driver::failure(&node).await;

    assert_eq!(only.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_three_clients_when_the_first_two_fail_then_the_third_answers_and_each_is_called_once()
 {
    let rig = Rig::new();
    let first = Model::new(vec![Act::fail(unavailable())]);
    let second = Model::new(vec![Act::fail(ClientError::Transport)]);
    let third = Model::new(vec![Act::text("third")]);
    let node = node(&rig, plains(&[&first, &second, &third]));

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "third");
    assert_eq!((first.calls(), second.calls(), third.calls()), (1, 1, 1));
}

#[tokio::test(start_paused = true)]
async fn given_three_clients_when_all_fail_then_each_is_called_once_and_the_last_error_is_the_fault()
 {
    let rig = Rig::new();
    let first = Model::new(vec![Act::fail(ClientError::Authentication)]);
    let second = Model::new(vec![Act::fail(ClientError::Transport)]);
    let third = Model::new(vec![Act::fail(ClientError::QuotaExceeded)]);
    let node = node(&rig, plains(&[&first, &second, &third]));

    let failure = Driver::failure(&node).await;

    assert_eq!((first.calls(), second.calls(), third.calls()), (1, 1, 1));
    assert!(failure.contains("the quota is exhausted"), "{failure}");
    assert!(!failure.contains("authentication failed"), "{failure}");
}

#[tokio::test(start_paused = true)]
async fn given_a_client_that_worked_when_later_steps_run_then_the_instance_keeps_that_client() {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]);
    let first = Model::new(vec![Act::fail(unavailable())]);
    let second = Model::new(vec![
        Act::call("alpha"),
        Act::call("alpha"),
        Act::text("done"),
    ]);
    let third = Model::new(vec![]);
    let node = node(&rig, plains(&[&first, &second, &third]));

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    assert_eq!((first.calls(), second.calls(), third.calls()), (1, 3, 0));
}

#[tokio::test(start_paused = true)]
async fn given_a_first_client_that_works_when_later_steps_run_then_the_others_are_never_called() {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]);
    let first = Model::new(vec![Act::call("alpha"), Act::text("done")]);
    let second = Model::new(vec![]);
    let node = node(&rig, plains(&[&first, &second]));

    Driver::result(&node).await;

    assert_eq!((first.calls(), second.calls()), (2, 0));
}

#[tokio::test(start_paused = true)]
async fn given_a_kept_client_when_a_later_step_fails_then_the_instance_fails_without_a_switch() {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]);
    let first = Model::new(vec![Act::call("alpha"), Act::fail(unavailable())]);
    let second = Model::new(vec![Act::text("never")]);
    let node = node(&rig, plains(&[&first, &second]));

    Driver::failure(&node).await;

    assert_eq!((first.calls(), second.calls()), (2, 0));
}

#[tokio::test(start_paused = true)]
async fn given_a_kept_client_when_a_later_step_times_out_then_the_instance_fails_without_a_switch()
{
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]);
    let first = Model::new(vec![Act::call("alpha"), Act::hang()]);
    let second = Model::new(vec![Act::text("never")]);
    let node = node(&rig, plains(&[&first, &second]));

    let failure = Driver::failure(&node).await;

    assert_eq!((first.calls(), second.calls()), (2, 0));
    assert!(failure.contains("timeout"), "{failure}");
}

#[tokio::test(start_paused = true)]
async fn given_a_client_error_of_refusal_kind_when_the_list_has_a_next_client_then_the_next_client_answers()
 {
    let rig = Rig::new();
    let first = Model::new(vec![Act::fail(ClientError::ContentBlocked)]);
    let second = Model::new(vec![Act::text("fine")]);
    let node = node(&rig, plains(&[&first, &second]));

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "fine");
    assert_eq!(update.steps, Some(1));
}

#[tokio::test(start_paused = true)]
async fn given_a_client_error_of_refusal_kind_when_the_client_is_alone_then_the_instance_fails() {
    let rig = Rig::new();
    let only = Model::new(vec![Act::fail(ClientError::ContentBlocked)]);
    let node = node(&rig, plains(&[&only]));

    let failure = Driver::failure(&node).await;

    assert!(failure.contains("the content was blocked"), "{failure}");
    assert_eq!(only.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_fallback_when_a_client_fails_then_the_switch_costs_no_step() {
    let rig = Rig::new();
    let first = Model::new(vec![Act::fail(unavailable())]);
    let second = Model::new(vec![Act::text("fine")]);
    let capture = Capture::new(&rig.record);
    let node = node_capturing(&rig, plains(&[&first, &second]), &capture);

    Driver::run(&node).await;

    assert_eq!(capture.only().steps(), 1);
}
