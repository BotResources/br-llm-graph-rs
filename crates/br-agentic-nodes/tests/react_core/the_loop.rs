use br_llm_messages::{ClosureReason, ResponseEnding};

use crate::common::*;
use crate::kit::*;

async fn ended_by(ending: ResponseEnding) -> (Recorded, Rig, std::sync::Arc<Model>) {
    let rig = Rig::new();
    let model = Model::new(vec![Act::ended("the end", ending), Act::text("never")]);
    let node = node(&rig, plains(&[&model]));
    let update = Driver::result(&node).await;
    (update, rig, model)
}

fn completed(reason: &Option<ClosureReason>) -> bool {
    matches!(reason, Some(ClosureReason::Completed { .. }))
}

#[tokio::test(start_paused = true)]
async fn given_a_final_response_when_the_step_ends_then_the_loop_ends_with_no_tool_run() {
    let rig = Rig::new();
    let model = Model::new(vec![Act::text("hello"), Act::text("never")]);
    let node = node(&rig, plains(&[&model]));

    let update = Driver::result(&node).await;

    assert_eq!((update.text.as_str(), update.steps), ("hello", Some(1)));
    assert_eq!(model.calls(), 1);
    assert_eq!(count(&rig.record, "open_result"), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_provider_continuation_when_the_step_ends_then_the_next_step_follows_with_no_tool_run()
 {
    let rig = Rig::new();
    let model = Model::new(vec![Act::continuation(), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    let update = Driver::result(&node).await;

    assert_eq!((update.text.as_str(), update.steps), ("done", Some(2)));
    assert!(history_text(&model.requests()[1]).contains("continuing"));
    assert_eq!(count(&rig.record, "open_result"), 0);
    let conversation = rig.conversation();
    assert_eq!(ai_turns(&conversation).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_pending_provider_execution_when_the_step_ends_then_the_next_step_follows() {
    let rig = Rig::new();
    let model = Model::new(vec![Act::pending_execution(), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    let update = Driver::result(&node).await;

    assert_eq!((update.text.as_str(), update.steps), ("done", Some(2)));
    assert!(history_text(&model.requests()[1]).contains("searching"));
}

#[tokio::test(start_paused = true)]
async fn given_runtime_tool_calls_with_a_provider_continuation_when_the_step_ends_then_the_tools_run_and_the_next_step_follows()
 {
    let (tool, probe) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]);
    let parts = vec![Part::Call("alpha".to_owned(), "{}".to_owned())];
    let model = Model::new(vec![
        Act::shaped(parts, ResponseEnding::provider_continuation()),
        Act::text("done"),
    ]);
    let node = node(&rig, plains(&[&model]));

    let update = Driver::result(&node).await;

    assert_eq!(*probe.starts.lock().unwrap(), 1);
    assert_eq!(update.steps, Some(2));
    assert!(history_text(&model.requests()[1]).contains("alpha done"));
}

#[tokio::test(start_paused = true)]
async fn given_tool_calls_when_the_tool_run_ends_then_the_next_step_is_made_on_the_conversation_with_the_results()
 {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]);
    let model = Model::new(vec![Act::call("alpha"), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    let update = Driver::result(&node).await;

    assert_eq!(update.steps, Some(2));
    let conversation = rig.conversation();
    let turns = ai_turns(&conversation);
    assert_eq!(turns.len(), 1);
    assert_eq!(responses(turns[0]).len(), 2);
    assert_eq!(results(turns[0]).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_refusal_ending_without_tool_call_when_the_step_ends_then_it_is_a_final_response() {
    let (update, rig, model) = ended_by(ResponseEnding::refusal()).await;

    assert_eq!(
        (update.text.as_str(), update.steps, update.step_limit),
        ("the end", Some(1), false)
    );
    assert_eq!(model.calls(), 1);
    assert!(completed(&closure(ai_turns(&rig.conversation())[0])));
}

#[tokio::test(start_paused = true)]
async fn given_a_filter_ending_without_tool_call_when_the_step_ends_then_it_is_a_final_response() {
    let (update, rig, model) = ended_by(ResponseEnding::filtering()).await;

    assert_eq!(
        (update.text.as_str(), update.steps, update.step_limit),
        ("the end", Some(1), false)
    );
    assert_eq!(model.calls(), 1);
    assert!(completed(&closure(ai_turns(&rig.conversation())[0])));
}

#[tokio::test(start_paused = true)]
async fn given_an_exhaustion_ending_without_tool_call_when_the_step_ends_then_it_is_a_final_response()
 {
    let (update, rig, model) = ended_by(ResponseEnding::output_exhaustion()).await;

    assert_eq!(
        (update.text.as_str(), update.steps, update.step_limit),
        ("the end", Some(1), false)
    );
    assert_eq!(model.calls(), 1);
    assert!(completed(&closure(ai_turns(&rig.conversation())[0])));
}
