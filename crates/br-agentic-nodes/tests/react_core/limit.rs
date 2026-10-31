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
async fn given_a_limit_of_three_when_every_response_asks_for_tools_then_the_third_is_stored_not_run_and_the_turn_is_budget_exhausted()
 {
    let record_rig = Rig::new().with_step_limit(3);
    let (alpha, spy_alpha) = spy(
        "alpha",
        Plan::Text("alpha done".to_owned()),
        &record_rig.record,
    );
    let rig = record_rig.with_tools(vec![alpha]);
    let model = Model::new(vec![
        Act::call("alpha"),
        Act::call("alpha"),
        Act::call("alpha"),
        Act::text("never"),
    ]);
    let capture = Capture::new(&rig.record);
    let node = node_capturing(&rig, plains(&[&model]), &capture);

    let update = Driver::result(&node).await;

    let requests = model.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(spy_alpha.starts(), 2);
    assert_eq!(count(&rig.record, "open_result"), 2);
    let conversation = rig.conversation();
    let turns = ai_turns(&conversation);
    assert_eq!(turns.len(), 1);
    assert_eq!(response_ids(&conversation).len(), 3);
    assert_eq!(results(turns[0]).len(), 2);
    let last = turns[0].state().closure().cloned();
    assert!(exhausted(&last));
    assert!(!last.unwrap().detail().unwrap_or("").trim().is_empty());
    let outcome = capture.only();
    assert_eq!(outcome.steps(), 3);
    assert!(outcome.step_limit_reached());
    assert_eq!(outcome.response().id(), requests[2].reply_entry());
    assert!(update.step_limit);
}

#[tokio::test(start_paused = true)]
async fn given_a_limit_of_one_when_the_response_asks_for_tools_then_no_tool_runs() {
    let record_rig = Rig::new().with_step_limit(1);
    let (alpha, spy_alpha) = spy(
        "alpha",
        Plan::Text("alpha done".to_owned()),
        &record_rig.record,
    );
    let rig = record_rig.with_tools(vec![alpha]);
    let model = Model::new(vec![Act::call("alpha")]);
    let node = node(&rig, plains(&[&model]));

    let update = Driver::result(&node).await;

    assert_eq!(spy_alpha.starts(), 0);
    assert_eq!(count(&rig.record, "open_result"), 0);
    assert_eq!((update.steps, update.step_limit), (Some(1), true));
    let conversation = rig.conversation();
    assert!(exhausted(&closure(ai_turns(&conversation)[0])));
}

#[tokio::test(start_paused = true)]
async fn given_a_limit_when_it_is_reached_then_the_model_is_not_called_again() {
    let (alpha, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![alpha]).with_step_limit(2);
    let model = Model::new(vec![
        Act::call("alpha"),
        Act::call("alpha"),
        Act::text("never"),
    ]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    assert_eq!(model.calls(), 2);
}

#[tokio::test(start_paused = true)]
async fn given_a_final_response_at_the_last_step_when_the_loop_ends_then_the_turn_is_completed_and_the_flag_is_not_set()
 {
    let (alpha, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![alpha]).with_step_limit(2);
    let model = Model::new(vec![Act::call("alpha"), Act::text("done")]);
    let capture = Capture::new(&rig.record);
    let node = node_capturing(&rig, plains(&[&model]), &capture);

    Driver::result(&node).await;

    let outcome = capture.only();
    assert_eq!((outcome.steps(), outcome.step_limit_reached()), (2, false));
    let conversation = rig.conversation();
    assert!(completed(&closure(ai_turns(&conversation)[0])));
}

#[tokio::test(start_paused = true)]
async fn given_a_provider_continuation_at_the_last_step_when_the_loop_ends_then_the_limit_stopped_the_work()
 {
    let rig = Rig::new().with_step_limit(1);
    let model = Model::new(vec![Act::continuation()]);
    let node = node(&rig, plains(&[&model]));

    let update = Driver::result(&node).await;

    assert!(update.step_limit);
    assert!(exhausted(&closure(ai_turns(&rig.conversation())[0])));
}

#[tokio::test(start_paused = true)]
async fn given_a_pending_provider_execution_at_the_last_step_when_the_loop_ends_then_the_limit_stopped_the_work()
 {
    let rig = Rig::new().with_step_limit(1);
    let model = Model::new(vec![Act::pending_execution()]);
    let node = node(&rig, plains(&[&model]));

    let update = Driver::result(&node).await;

    assert!(update.step_limit);
    assert!(exhausted(&closure(ai_turns(&rig.conversation())[0])));
}

#[tokio::test(start_paused = true)]
async fn given_a_restart_after_a_cancel_when_the_new_instance_runs_then_its_step_counter_starts_at_zero()
 {
    let record_rig = Rig::new().with_step_limit(3);
    let (alpha, _) = spy(
        "alpha",
        Plan::Text("alpha done".to_owned()),
        &record_rig.record,
    );
    let rig = record_rig.with_tools(vec![alpha]);
    let first = Model::new(vec![Act::call("alpha"), Act::call("alpha"), Act::hang()]);
    let before = node(&rig, plains(&[&first]));
    let mut driver = Driver::start(&before).await;
    driver.give();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    driver.cancel.handle().trigger();
    assert!(matches!(
        driver.event().await,
        br_graph_contract::Event::Cancelled
    ));
    let second = Model::new(vec![
        Act::call("alpha"),
        Act::call("alpha"),
        Act::text("done"),
    ]);
    let capture = Capture::new(&rig.record);
    let after = node_capturing(&rig, plains(&[&second]), &capture);

    Driver::result(&after).await;

    let outcome = capture.only();
    assert_eq!((outcome.steps(), outcome.step_limit_reached()), (3, false));
    assert_eq!(first.calls(), 3);
}
