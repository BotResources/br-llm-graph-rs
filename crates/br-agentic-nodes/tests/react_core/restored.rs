use br_agentic_contract::WorkspaceFile;
use br_llm_messages::{ClosureReason, IncomingEntry, InputTurn, ResponseEnding, Turn};

use crate::common::*;
use crate::kit::*;

fn file(path: &str, content: &str) -> WorkspaceFile {
    WorkspaceFile {
        path: path.to_owned(),
        content: content.as_bytes().to_vec(),
    }
}

fn interrupted(reason: &Option<ClosureReason>) -> bool {
    matches!(reason, Some(ClosureReason::Interrupted { .. }))
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_final_response_in_an_open_turn_when_the_instance_starts_then_it_makes_no_step_and_keeps_the_interrupted_reason()
 {
    let rig = seeded(|rig, conversation| {
        seed_turn(
            conversation,
            rig.agent,
            text_part("restored"),
            ResponseEnding::finished(),
            None,
        );
    });
    let model = Model::new(vec![]);
    let capture = Capture::new(&rig.record);
    let node = node_capturing(&rig, plains(&[&model]), &capture);

    let update = Driver::result(&node).await;

    let outcome = capture.only();
    assert_eq!((outcome.steps(), outcome.step_limit_reached()), (0, false));
    assert_eq!(update.text, "restored");
    assert_eq!(model.calls(), 0);
    let conversation = rig.conversation();
    let turns = ai_turns(&conversation);
    assert_eq!(turns.len(), 1);
    assert!(interrupted(&closure(turns[0])));
    assert_eq!(closures_given(&rig.record).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_final_response_in_a_completed_turn_when_the_instance_starts_then_it_ends_with_it_and_closes_nothing()
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
    let node = node_capturing(&rig, plains(&[&model]), &capture);

    Driver::result(&node).await;

    let outcome = capture.only();
    assert_eq!((outcome.steps(), outcome.step_limit_reached()), (0, false));
    assert_eq!(model.calls(), 0);
    assert!(closures_given(&rig.record).is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_budget_exhausted_turn_when_the_instance_starts_then_it_makes_no_step_and_the_flag_is_set()
 {
    let rig = seeded(|rig, conversation| {
        let call = vec![Part::Call("alpha".to_owned(), "{}".to_owned())];
        let closure = ClosureReason::budget_exhausted();
        seed_turn(
            conversation,
            rig.agent,
            call,
            ResponseEnding::tool_use(),
            Some(closure),
        );
    });
    let (tool, spy_alpha) = spy("alpha", Plan::Text("alpha done".to_owned()), &rig.record);
    let rig = rig.with_tools(vec![tool]);
    let model = Model::new(vec![]);
    let capture = Capture::new(&rig.record);
    let node = node_capturing(&rig, plains(&[&model]), &capture);

    let update = Driver::result(&node).await;

    let outcome = capture.only();
    assert_eq!((outcome.steps(), outcome.step_limit_reached()), (0, true));
    assert!(update.step_limit);
    assert_eq!(model.calls(), 0);
    assert_eq!(spy_alpha.starts(), 0);
    assert_eq!(count(&rig.record, "open_result"), 0);
    assert!(closures_given(&rig.record).is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_final_response_followed_by_a_new_input_when_the_instance_starts_then_a_step_is_made()
 {
    let rig = seeded(|rig, conversation| {
        seed_turn(
            conversation,
            rig.agent,
            text_part("old"),
            ResponseEnding::finished(),
            Some(ClosureReason::completed()),
        );
        let entry = IncomingEntry::from(human_message("again"));
        let turn = InputTurn::new(turn_id(), vec![entry]).unwrap();
        conversation.push_turn(Turn::from(turn)).unwrap();
    });
    let model = Model::new(vec![Act::text("new")]);
    let node = node(&rig, plains(&[&model]));

    let update = Driver::result(&node).await;

    assert_eq!(model.calls(), 1);
    assert_eq!(update.text, "new");
    assert_eq!(update.steps, Some(1));
}

#[tokio::test(start_paused = true)]
async fn given_a_final_response_of_another_agent_as_last_turn_when_the_instance_starts_then_a_step_is_made()
 {
    let rig = seeded(|_, conversation| {
        seed_turn(
            conversation,
            agent_id(),
            text_part("theirs"),
            ResponseEnding::finished(),
            Some(ClosureReason::completed()),
        );
    });
    let model = Model::new(vec![Act::text("mine")]);
    let node = node(&rig, plains(&[&model]));

    let update = Driver::result(&node).await;

    assert_eq!(model.calls(), 1);
    assert_eq!(update.text, "mine");
}
