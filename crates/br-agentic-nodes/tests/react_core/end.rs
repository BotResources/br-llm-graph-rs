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
