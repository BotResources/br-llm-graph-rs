use br_llm_messages::TurnEntry;

use crate::common::*;
use crate::kit::*;

fn calls_then_answer(calls: usize) -> Vec<Act> {
    let mut acts: Vec<Act> = (0..calls).map(|_| Act::call("alpha")).collect();
    acts.push(Act::text("done"));
    acts
}

async fn hundred_steps() -> (Rig, std::sync::Arc<Model>) {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]).with_step_limit(100);
    let model = Model::new(calls_then_answer(99));
    let node = node(&rig, plains(&[&model]));
    Driver::result(&node).await;
    (rig, model)
}

#[tokio::test(start_paused = true)]
async fn given_a_limit_of_one_hundred_when_the_tool_run_of_step_ninety_ends_then_the_warning_first_appears_in_step_ninety_one()
 {
    let (_, model) = hundred_steps().await;

    let requests = model.requests();
    assert_eq!(requests.len(), 100);
    assert!((0..90).all(|index| warning_of(&requests[index]).is_none()));
    let first = warning_of(&requests[90]).unwrap();
    assert!(first.contains("10"), "{first}");
}

#[tokio::test(start_paused = true)]
async fn given_a_warning_when_each_later_tool_run_ends_then_it_is_repeated_and_tells_the_steps_left()
 {
    let (_, model) = hundred_steps().await;

    let requests = model.requests();
    for (index, request) in requests.iter().enumerate().take(98).skip(90) {
        let left = 100 - index;
        let text = warning_of(request).unwrap();
        assert!(
            text.contains(&left.to_string()),
            "step {}: {text}",
            index + 1
        );
    }
}

#[tokio::test(start_paused = true)]
async fn given_one_step_left_when_the_step_is_called_then_the_warning_asks_for_no_tool_call_and_the_tools_are_still_offered()
 {
    let (_, model) = hundred_steps().await;

    let last = model.requests()[99].clone();
    let text = warning_of(&last).unwrap().to_lowercase();
    assert!(text.contains("no tool"), "{text}");
    assert_ne!(
        Some(text),
        warning_of(&model.requests()[98]).map(|other| other.to_lowercase())
    );
    assert_eq!(last.tools().len(), 1);
    assert_eq!(last.tools()[0].name().as_str(), "alpha");
}

#[tokio::test(start_paused = true)]
async fn given_warnings_when_the_run_ends_then_the_text_is_stored_nowhere() {
    let (rig, _) = hundred_steps().await;

    let conversation = rig.conversation();
    let stored = ai_turns(&conversation)
        .into_iter()
        .flat_map(|turn| turn.entries().iter())
        .filter(|entry| matches!(entry, TurnEntry::RuntimeNotice(_)))
        .count();
    assert_eq!(stored, 0);
    assert_eq!(count(&rig.record, "add_entry"), 0);
    assert!(
        input_turns(&conversation)
            .iter()
            .all(|turn| turn.entries().len() == 1)
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_small_limit_when_the_first_step_runs_then_it_has_no_warning_and_the_next_one_after_a_tool_run_has_it()
 {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]).with_step_limit(5);
    let model = Model::new(calls_then_answer(1));
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    let requests = model.requests();
    assert!(warning_of(&requests[0]).is_none());
    let text = warning_of(&requests[1]).unwrap();
    assert!(text.contains('4'), "{text}");
}

#[tokio::test(start_paused = true)]
async fn given_a_response_with_a_provider_continuation_when_the_next_step_is_called_then_it_has_no_warning()
 {
    let rig = Rig::new().with_step_limit(5);
    let model = Model::new(vec![Act::continuation(), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    assert!(warning_of(&model.requests()[1]).is_none());
}

#[tokio::test(start_paused = true)]
async fn given_a_response_with_a_pending_provider_execution_when_the_next_step_is_called_then_it_has_no_warning()
 {
    let rig = Rig::new().with_step_limit(5);
    let model = Model::new(vec![Act::pending_execution(), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    assert!(warning_of(&model.requests()[1]).is_none());
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_run_then_a_continuation_when_the_steps_follow_then_only_the_step_after_the_tool_run_has_the_warning()
 {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]).with_step_limit(6);
    let model = Model::new(vec![
        Act::call("alpha"),
        Act::continuation(),
        Act::text("done"),
    ]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    let requests = model.requests();
    assert!(warning_of(&requests[1]).is_some());
    assert!(warning_of(&requests[2]).is_none());
}

#[tokio::test(start_paused = true)]
async fn given_a_limit_of_twelve_when_eleven_steps_remain_then_there_is_no_warning_and_with_ten_there_is()
 {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]).with_step_limit(12);
    let model = Model::new(calls_then_answer(2));
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    let requests = model.requests();
    assert!(warning_of(&requests[1]).is_none());
    assert!(warning_of(&requests[2]).unwrap().contains("10"));
}
