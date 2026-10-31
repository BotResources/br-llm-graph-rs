use std::time::Duration;

use br_graph_contract::Event;

use crate::common::*;
use crate::kit::*;
use crate::spy::{Items, Spy};

fn rig_with_noop() -> Rig {
    let (noop, _) = echo("noop");
    Rig::new().with_tools(vec![noop])
}

fn rounds(count: usize) -> Vec<Reply> {
    let mut replies: Vec<Reply> = (0..count).map(|_| Reply::call("noop")).collect();
    replies.push(Reply::text("done"));
    replies
}

fn names_of(rig: &Rig) -> Vec<&'static str> {
    names(&rig.record)
}

#[tokio::test(start_paused = true)]
async fn given_a_failing_first_read_when_the_agent_starts_then_the_run_goes_on_without_fault() {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    spy.plan_items(vec![Items::Fail]);
    let model = Scripted::new(vec![Reply::text("ok")]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "ok");
    assert_eq!(update.steps, Some(1));
}

#[tokio::test(start_paused = true)]
async fn given_a_first_read_that_fails_after_a_batch_when_the_agent_starts_then_it_stores_the_items_it_holds()
 {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let (a, a_id) = human("kept-after-failure");
    spy.plan_items(vec![
        Items::Give(batch(vec![item(&a)], Some("c1"))),
        Items::Fail,
    ]);
    let model = Scripted::new(vec![Reply::text("ok")]);
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;

    assert_eq!(count_of(&rig.conversation(), a_id), 1);
    assert!(history_text(&model.requests()[0]).contains("kept-after-failure"));
}

#[tokio::test(start_paused = true)]
async fn given_a_later_read_that_fails_when_the_next_boundary_comes_then_another_read_starts_and_its_item_is_stored()
 {
    let rig = rig_with_noop();
    let spy = Spy::new(&rig);
    let (x, x_id) = human("arrives-after-a-failed-read");
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Fail,
        Items::Give(batch(vec![item(&x)], None)),
    ]);
    let model = Scripted::new(rounds(3));
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    assert_eq!(count_of(&rig.conversation(), x_id), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_running_read_when_the_instance_ends_then_the_read_is_dropped() {
    let rig = rig_with_noop();
    let spy = Spy::new(&rig);
    spy.plan_items(vec![Items::Pass, Items::Hang]);
    let model = Scripted::new(rounds(2));
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;
    settle().await;

    assert_eq!(spy.dropped_reads(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_running_read_when_the_instance_fails_then_the_read_is_dropped() {
    let rig = rig_with_noop().with_outcome(Recording::failing());
    let spy = Spy::new(&rig);
    spy.plan_items(vec![Items::Pass, Items::Hang]);
    let model = Scripted::new(rounds(2));
    let node = spied(&rig, &spy, &model);

    let cause = Driver::failure(&node).await;
    settle().await;

    assert!(cause.contains("outcome port failed"), "{cause}");
    assert_eq!(spy.dropped_reads(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_running_read_when_the_instance_is_cancelled_then_the_read_is_dropped() {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    spy.plan_items(vec![Items::Pass, Items::Hang]);
    let model = Scripted::new(vec![Reply::Hang]);
    let node = spied(&rig, &spy, &model);
    let mut driver = Driver::start(&node).await;
    driver.give();
    tokio::time::sleep(Duration::from_millis(100)).await;

    driver.cancel.handle().trigger();
    let event = driver.event().await;
    settle().await;

    assert!(matches!(event, Event::Cancelled));
    assert_eq!(spy.dropped_reads(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_read_that_never_ends_when_the_agent_finishes_then_it_does_not_wait_for_the_read() {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    spy.plan_items(vec![Items::Pass, Items::Hang]);
    let model = Scripted::new(vec![Reply::text("ok")]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "ok");
    assert_eq!(spy.reads(), 2);
}
