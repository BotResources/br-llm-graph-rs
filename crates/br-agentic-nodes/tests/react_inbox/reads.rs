use std::time::Duration;

use tokio::time::Instant;

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
async fn given_a_slow_first_read_when_the_agent_starts_then_the_first_request_holds_the_item() {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let (entry, _) = human("marker-awaited-at-start");
    rig.inbox.deliver_entry(rig.agent, entry);
    spy.plan_items(vec![Items::Sleep(Duration::from_secs(5))]);
    let model = Scripted::new(vec![Reply::text("ok")]);
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;

    assert!(history_text(&model.requests()[0]).contains("marker-awaited-at-start"));
    let calls = names_of(&rig);
    let read = calls
        .iter()
        .position(|name| *name == "inbox_items")
        .unwrap();
    let step = calls
        .iter()
        .position(|name| *name == "open_response")
        .unwrap();
    assert!(read < step);
}

#[tokio::test(start_paused = true)]
async fn given_a_read_that_stays_slow_when_boundaries_pass_then_no_second_read_runs_and_the_loop_never_waits()
 {
    let rig = rig_with_noop();
    let spy = Spy::new(&rig);
    spy.plan_items(vec![Items::Pass, Items::Sleep(Duration::from_secs(1000))]);
    let model = Scripted::new(rounds(4));
    let node = spied(&rig, &spy, &model);
    let began = Instant::now();

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    assert!(began.elapsed() < Duration::from_secs(1000));
    assert_eq!(spy.reads(), 2);
    assert_eq!(spy.peak_reads(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_reads_that_end_quickly_when_boundaries_pass_then_one_new_read_starts_at_each_boundary()
 {
    let rig = rig_with_noop();
    let spy = Spy::new(&rig);
    let model = Scripted::new(rounds(4));
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;

    assert_eq!(spy.reads(), 1 + 1 + 4);
    assert_eq!(spy.peak_reads(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_an_inbox_when_the_agent_runs_then_the_wait_of_the_inbox_is_never_used() {
    let rig = rig_with_noop();
    let spy = Spy::new(&rig);
    let model = Scripted::new(rounds(3));
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;

    assert_eq!(spy.waits(), 0);
    assert!(!names_of(&rig).contains(&"inbox_wait"));
}

#[tokio::test(start_paused = true)]
async fn given_batches_with_cursors_when_the_agent_starts_then_it_reads_until_no_cursor_and_stores_in_order()
 {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let (a, a_id) = human("batch-a");
    let (b, b_id) = human("batch-b");
    let (c, c_id) = human("batch-c");
    spy.plan_items(vec![
        Items::Give(batch(vec![item(&a)], Some("c1"))),
        Items::Give(batch(vec![item(&b)], Some("c2"))),
        Items::Give(batch(vec![item(&c)], None)),
    ]);
    let model = Scripted::new(vec![Reply::text("ok")]);
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;

    let cursors = spy.cursors();
    assert_eq!(
        cursors[..3],
        [None, Some("c1".to_owned()), Some("c2".to_owned())]
    );
    let conversation = rig.conversation();
    let turns = input_turns(&conversation);
    let stored: Vec<_> = turns[1].entries().iter().map(|entry| entry.id()).collect();
    assert_eq!(stored, vec![a_id, b_id, c_id]);
}

#[tokio::test(start_paused = true)]
async fn given_items_in_several_batches_when_the_agent_runs_then_the_order_of_arrival_is_kept() {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let (a, a_id) = human("first-in-first-batch");
    let (b, b_id) = human("second-in-first-batch");
    let (c, c_id) = human("only-in-second-batch");
    spy.plan_items(vec![
        Items::Give(batch(vec![item(&a), item(&b)], Some("next"))),
        Items::Give(batch(vec![item(&c)], None)),
    ]);
    let model = Scripted::new(vec![Reply::text("ok")]);
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;

    let conversation = rig.conversation();
    let turns = input_turns(&conversation);
    let stored: Vec<_> = turns[1].entries().iter().map(|entry| entry.id()).collect();
    assert_eq!(stored, vec![a_id, b_id, c_id]);
}

#[tokio::test(start_paused = true)]
async fn given_a_drain_when_a_new_read_starts_then_its_items_wait_for_the_next_boundary() {
    let rig = rig_with_noop();
    let spy = Spy::new(&rig);
    let (a, a_id) = human("queued-at-the-first-boundary");
    let (b, b_id) = human("read-during-the-first-boundary");
    rig.inbox.deliver_entry(rig.agent, b);
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Give(batch(vec![item(&a)], None)),
    ]);
    let model = Scripted::new(rounds(3));
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;

    let conversation = rig.conversation();
    let turn = ai_turns(&conversation)[0];
    let first = results(turn)[0].id();
    let second = results(turn)[1].id();
    let at = |id| position_of(&conversation, id).unwrap();
    assert!(at(first) < at(a_id));
    assert!(at(a_id) < at(second));
    assert!(at(second) < at(b_id));
}

#[tokio::test(start_paused = true)]
async fn given_an_item_that_arrives_while_a_read_runs_when_the_agent_ends_then_the_item_stays_in_the_inbox()
 {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    spy.plan_items(vec![Items::Pass, Items::Sleep(Duration::from_secs(1000))]);
    let (x, x_id) = human("arrives-too-late");
    let inbox = rig.inbox.clone();
    let agent = rig.agent;
    let model = Scripted::new(vec![Reply::text("ok")]);
    on_nth_call(&model, 1, move || inbox.deliver_entry(agent, x.clone()));
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.steps, Some(1));
    assert_eq!(count_of(&rig.conversation(), x_id), 0);
    assert!(!rig.inbox.is_read(rig.agent, x_id));
}

#[tokio::test(start_paused = true)]
async fn given_no_inbox_when_the_agent_runs_then_the_inbox_is_never_called_and_nothing_is_stored() {
    let rig = rig_with_noop();
    let (entry, x_id) = human("not-for-a-node-without-inbox");
    rig.inbox.deliver_entry(rig.agent, entry);
    let model = Scripted::new(rounds(2));
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert!(!names_of(&rig).iter().any(|name| name.starts_with("inbox")));
    assert_eq!(count_of(&rig.conversation(), x_id), 0);
    assert!(!rig.inbox.is_read(rig.agent, x_id));
}
