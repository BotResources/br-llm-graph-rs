use br_agentic_contract::FileSystemMode;
use br_llm_messages::{ClosureReason, ResponseEnding};

use crate::common::*;
use crate::kit::*;

fn exhausted(reason: &Option<ClosureReason>) -> bool {
    matches!(reason, Some(ClosureReason::BudgetExhausted { .. }))
}

fn completed(reason: &Option<ClosureReason>) -> bool {
    matches!(reason, Some(ClosureReason::Completed { .. }))
}

#[tokio::test(start_paused = true)]
async fn given_a_final_response_when_the_instance_ends_then_the_outcome_holds_the_last_response_the_steps_and_the_flag()
 {
    let (alpha, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![alpha]);
    let model = Model::new(vec![Act::call("alpha"), Act::text("done")]);
    let capture = Capture::new(&rig.record);
    let node = node_capturing(&rig, plains(&[&model]), &capture);

    let update = Driver::result(&node).await;

    let outcome = capture.only();
    assert_eq!(outcome.response().id(), model.requests()[1].reply_entry());
    assert_eq!((outcome.steps(), outcome.step_limit_reached()), (2, false));
    assert_eq!(update.text, "done");
    assert_eq!(capture.seen.lock().unwrap()[0].note, "start");
}

#[tokio::test(start_paused = true)]
async fn given_a_final_response_when_the_instance_ends_then_the_turn_is_completed_before_the_outcome_port_is_called()
 {
    let rig = Rig::new();
    let model = Model::new(vec![Act::text("done")]);
    let capture = Capture::new(&rig.record);
    let node = node_capturing(&rig, plains(&[&model]), &capture);

    Driver::result(&node).await;

    let before = capture.seen.lock().unwrap()[0].before.clone();
    assert!(before.contains(&"close_turn"));
    assert!(completed(&closure(ai_turns(&rig.conversation())[0])));
    assert_eq!(closures_given(&rig.record).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_file_system_when_the_instance_ends_with_a_result_then_the_files_are_saved_once_after_the_outcome_port()
 {
    let files = vec![br_agentic_contract::WorkspaceFile {
        path: "a.txt".to_owned(),
        content: b"a".to_vec(),
    }];
    let rig = Rig::new().with_files(FileSystemMode::Always, files.clone());
    let model = Model::new(vec![Act::text("done")]);
    let capture = Capture::new(&rig.record);
    let node = node_capturing(&rig, plains(&[&model]), &capture);

    Driver::result(&node).await;

    assert_eq!(saves(&rig.record), vec![files]);
    assert!(
        !capture.seen.lock().unwrap()[0]
            .before
            .contains(&"workspace_save")
    );
    assert!(position(&rig.record, "workspace_save") > last_position(&rig.record, "close_turn"));
    assert!(
        position(&rig.record, "workspace_save") > last_position(&rig.record, "complete_response")
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_file_system_when_the_run_has_several_steps_then_nothing_is_saved_before_the_end() {
    let (alpha, _) = echo("alpha");
    let rig = Rig::new()
        .with_tools(vec![alpha])
        .with_files(FileSystemMode::Always, Vec::new());
    let model = Model::new(vec![
        Act::call("alpha"),
        Act::call("alpha"),
        Act::text("done"),
    ]);
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let (record, sink) = (rig.record.clone(), seen.clone());
    model.on_call(move |_| sink.lock().unwrap().push(count(&record, "workspace_save")));
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    assert_eq!(seen.lock().unwrap().clone(), vec![0, 0, 0]);
    assert_eq!(count(&rig.record, "workspace_save"), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_file_system_when_the_step_limit_ends_the_instance_then_the_files_are_saved() {
    let rig = Rig::new()
        .with_step_limit(1)
        .with_files(FileSystemMode::Always, Vec::new());
    let model = Model::new(vec![Act::continuation()]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    assert_eq!(count(&rig.record, "workspace_save"), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_file_system_when_the_instance_fails_then_no_file_is_saved() {
    let record_rig = Rig::new();
    let (boom, _) = spy("boom", Plan::Fault, &record_rig.record);
    let rig = record_rig
        .with_tools(vec![boom])
        .with_files(FileSystemMode::Always, Vec::new());
    let model = Model::new(vec![Act::call("boom")]);
    let node = node(&rig, plains(&[&model]));

    Driver::failure(&node).await;

    assert_eq!(count(&rig.record, "workspace_save"), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_file_system_when_the_outcome_port_fails_then_the_instance_fails_and_no_file_is_saved()
 {
    let rig = Rig::new().with_files(FileSystemMode::Always, Vec::new());
    let model = Model::new(vec![Act::text("done")]);
    let capture = Capture::failing(&rig.record);
    let node = node_capturing(&rig, plains(&[&model]), &capture);

    Driver::failure(&node).await;

    assert_eq!(count(&rig.record, "workspace_save"), 0);
    assert_eq!(capture.attempts(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_file_system_when_the_instance_is_cancelled_then_no_file_is_saved() {
    let rig = Rig::new().with_files(FileSystemMode::Always, Vec::new());
    let model = Model::new(vec![Act::hang()]);
    let node = node(&rig, plains(&[&model]));
    let mut driver = Driver::start(&node).await;
    driver.give();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    driver.cancel.handle().trigger();

    let event = driver.event().await;

    assert!(matches!(event, br_graph_contract::Event::Cancelled));
    assert_eq!(count(&rig.record, "workspace_save"), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_refusal_ending_when_the_instance_ends_then_the_outcome_holds_that_response() {
    let rig = Rig::new();
    let model = Model::new(vec![Act::ended("no", ResponseEnding::refusal())]);
    let capture = Capture::new(&rig.record);
    let node = node_capturing(&rig, plains(&[&model]), &capture);

    Driver::result(&node).await;

    let outcome = capture.only();
    assert_eq!(outcome.response().ending(), &ResponseEnding::refusal());
    assert!(!outcome.step_limit_reached());
}
