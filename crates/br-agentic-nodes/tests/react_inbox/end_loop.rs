use br_llm_messages::{ClosureReason, IncomingEntry};

use crate::common::*;
use crate::kit::*;
use crate::spy::{Items, Spy};

#[tokio::test(start_paused = true)]
async fn given_an_item_queued_after_a_final_response_when_the_agent_ends_then_a_new_ai_turn_answers_it()
 {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let (x, x_id) = human("marker-after-the-answer");
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Give(batch(vec![item(&x)], None)),
    ]);
    let model = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "two");
    assert_eq!(update.steps, Some(2));
    assert!(!update.step_limit);
    let conversation = rig.conversation();
    let turns = ai_turns(&conversation);
    assert_eq!(turns.len(), 2);
    assert_eq!(responses(turns[0]), vec!["one".to_owned()]);
    assert_eq!(responses(turns[1]), vec!["two".to_owned()]);
    assert_eq!(closure(turns[0]), Some(ClosureReason::completed()));
    assert_eq!(closure(turns[1]), Some(ClosureReason::completed()));
    assert_ne!(turns[0].id(), turns[1].id());
    let inputs = input_turns(&conversation);
    assert_eq!(inputs.len(), 2);
    assert_eq!(
        inputs[1]
            .entries()
            .iter()
            .map(IncomingEntry::id)
            .collect::<Vec<_>>(),
        vec![x_id]
    );
    assert!(history_text(&model.requests()[1]).contains("marker-after-the-answer"));
}

#[tokio::test(start_paused = true)]
async fn given_an_item_queued_after_a_final_response_when_the_agent_ends_then_the_first_turn_is_closed_before_the_item_is_stored()
 {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let (x, x_id) = human("stored-after-the-closure");
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Give(batch(vec![item(&x)], None)),
    ]);
    let model = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;

    let log = steps(&rig.record);
    let closed = index_of(&log, &Step::Close);
    let stored = index_of(&log, &Step::AddTurn(vec![x_id]));
    assert!(closed < stored);
}

#[tokio::test(start_paused = true)]
async fn given_several_items_queued_after_a_final_response_when_the_agent_ends_then_one_input_turn_holds_one_entry_for_each()
 {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let (x, x_id) = human("first-queued-item");
    let (y, y_id) = human("second-queued-item");
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Give(batch(vec![item(&x), item(&y)], None)),
    ]);
    let model = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.steps, Some(2));
    let conversation = rig.conversation();
    assert_eq!(ai_turns(&conversation).len(), 2);
    let inputs = input_turns(&conversation);
    assert_eq!(inputs.len(), 2);
    assert_eq!(
        inputs[1]
            .entries()
            .iter()
            .map(IncomingEntry::id)
            .collect::<Vec<_>>(),
        vec![x_id, y_id]
    );
}

#[tokio::test(start_paused = true)]
async fn given_items_that_keep_coming_when_each_answer_ends_then_a_new_ai_turn_starts_until_no_item_is_new()
 {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let (x, _) = human("first-wave");
    let (y, _) = human("second-wave");
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Give(batch(vec![item(&x)], None)),
        Items::Give(batch(vec![item(&y)], None)),
    ]);
    let model = Scripted::new(vec![
        Reply::text("one"),
        Reply::text("two"),
        Reply::text("three"),
    ]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "three");
    assert_eq!(update.steps, Some(3));
    let conversation = rig.conversation();
    assert_eq!(ai_turns(&conversation).len(), 3);
    assert_eq!(input_turns(&conversation).len(), 3);
    assert_eq!(model.calls(), 3);
}

#[tokio::test(start_paused = true)]
async fn given_a_new_ai_turn_for_a_queued_item_when_the_step_limit_is_reached_then_no_further_drain_happens()
 {
    let rig = Rig::new().with_step_limit(2);
    let spy = Spy::new(&rig);
    let (x, x_id) = human("takes-the-last-step");
    let (y, y_id) = human("finds-no-step-left");
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Give(batch(vec![item(&x)], None)),
        Items::Give(batch(vec![item(&y)], None)),
    ]);
    let model = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.steps, Some(2));
    assert!(!update.step_limit);
    let conversation = rig.conversation();
    assert_eq!(count_of(&conversation, x_id), 1);
    assert_eq!(count_of(&conversation, y_id), 0);
    assert_eq!(spy.times_marked(y_id), 0);
    assert_eq!(ai_turns(&conversation).len(), 2);
}

#[tokio::test(start_paused = true)]
async fn given_only_items_the_conversation_holds_when_a_final_response_ends_then_they_are_marked_and_no_new_turn_starts()
 {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let (x, x_id) = human("held-and-given-again");
    spy.plan_items(vec![
        Items::Give(batch(vec![item(&x)], None)),
        Items::Give(batch(vec![item(&x)], None)),
    ]);
    let model = Scripted::new(vec![Reply::text("one")]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.steps, Some(1));
    let conversation = rig.conversation();
    assert_eq!(ai_turns(&conversation).len(), 1);
    assert_eq!(count_of(&conversation, x_id), 1);
    assert_eq!(spy.times_marked(x_id), 2);
    assert_eq!(model.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_queued_item_when_the_agent_has_no_inbox_then_the_answer_ends_the_instance() {
    let rig = Rig::new();
    let (x, x_id) = human("nobody-reads-this");
    rig.inbox.deliver_entry(rig.agent, x);
    let model = Scripted::new(vec![Reply::text("one")]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.steps, Some(1));
    assert_eq!(count_of(&rig.conversation(), x_id), 0);
}
