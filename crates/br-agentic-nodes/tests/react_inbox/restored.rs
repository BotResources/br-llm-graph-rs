use crate::common::*;
use crate::kit::*;
use crate::spy::{Items, Spy};

fn rig_with_noop(limit: u32) -> Rig {
    let (noop, _) = echo("noop");
    Rig::new().with_tools(vec![noop]).with_step_limit(limit)
}

async fn first_run(rig: &Rig, replies: Vec<Reply>) {
    let model = Scripted::new(replies);
    let node = plain(rig, &model, "Agent");
    Driver::result(&node).await;
}

#[tokio::test(start_paused = true)]
async fn given_a_restart_on_a_final_response_when_the_agent_starts_then_it_ends_with_no_step_and_waits_for_no_read()
 {
    let rig = rig_with_noop(20);
    first_run(&rig, vec![Reply::text("first answer")]).await;
    let (late, late_id) = human("arrives-after-the-read-at-start");
    rig.inbox.deliver_entry(rig.agent, late);
    let spy = Spy::new(&rig);
    spy.plan_items(vec![Items::Give(empty_batch()), Items::Hang]);
    let second = Scripted::new(Vec::new());
    let node = spied(&rig, &spy, &second);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "first answer");
    assert_eq!(update.steps, Some(0));
    assert!(!update.step_limit);
    assert_eq!(second.calls(), 0);
    assert_eq!(count_of(&rig.conversation(), late_id), 0);
    assert!(!rig.inbox.is_read(rig.agent, late_id));
    assert_eq!(ai_turns(&rig.conversation()).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_restart_on_a_budget_exhausted_turn_when_the_agent_starts_then_it_ends_with_no_step_and_the_flag_set()
 {
    let rig = rig_with_noop(1);
    first_run(&rig, vec![Reply::call("noop")]).await;
    let (late, late_id) = human("arrives-after-the-budget-ended");
    rig.inbox.deliver_entry(rig.agent, late);
    let spy = Spy::new(&rig);
    spy.plan_items(vec![Items::Give(empty_batch()), Items::Hang]);
    let second = Scripted::new(Vec::new());
    let node = spied(&rig, &spy, &second);

    let update = Driver::result(&node).await;

    assert_eq!(update.steps, Some(0));
    assert!(update.step_limit);
    assert_eq!(second.calls(), 0);
    assert_eq!(count_of(&rig.conversation(), late_id), 0);
    assert!(!rig.inbox.is_read(rig.agent, late_id));
}

#[tokio::test(start_paused = true)]
async fn given_a_restart_on_a_final_response_when_the_read_at_start_gives_an_item_then_the_item_is_stored_and_a_step_answers_it()
 {
    let rig = rig_with_noop(20);
    first_run(&rig, vec![Reply::text("first answer")]).await;
    let (late, late_id) = human("marker-waiting-at-restart");
    rig.inbox.deliver_entry(rig.agent, late);
    let second = Scripted::new(vec![Reply::text("second answer")]);
    let node = plain(&rig, &second, "Agent");

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "second answer");
    assert_eq!(update.steps, Some(1));
    assert_eq!(count_of(&rig.conversation(), late_id), 1);
    assert!(rig.inbox.is_read(rig.agent, late_id));
    assert!(history_text(&second.requests()[0]).contains("marker-waiting-at-restart"));
}

#[tokio::test(start_paused = true)]
async fn given_a_restart_on_a_final_response_when_the_read_at_start_fails_then_the_instance_still_ends_at_once()
 {
    let rig = rig_with_noop(20);
    first_run(&rig, vec![Reply::text("first answer")]).await;
    let spy = Spy::new(&rig);
    spy.plan_items(vec![Items::Fail]);
    let second = Scripted::new(Vec::new());
    let node = spied(&rig, &spy, &second);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "first answer");
    assert_eq!(update.steps, Some(0));
    assert_eq!(second.calls(), 0);
}
