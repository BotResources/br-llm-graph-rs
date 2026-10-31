use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_graph_contract::Event;
use tokio::time::Instant;

use crate::access::FailingAdds;
use crate::common::*;
use crate::kit::*;
use crate::spy::{Act, Items, Spy};

fn rig_with_noop() -> Rig {
    let (noop, _) = echo("noop");
    Rig::new().with_tools(vec![noop])
}

fn stamp_calls(model: &Arc<Scripted>) -> Arc<Mutex<Vec<Instant>>> {
    let stamps = Arc::new(Mutex::new(Vec::new()));
    let shared = stamps.clone();
    model.on_call(move |_| shared.lock().unwrap().push(Instant::now()));
    stamps
}

#[tokio::test(start_paused = true)]
async fn given_items_stored_in_the_open_turn_when_the_boundary_ends_then_each_mark_follows_its_addition_and_precedes_the_next()
 {
    let rig = rig_with_noop();
    let spy = Spy::new(&rig);
    let (a, a_id) = human("mark-order-a");
    let (b, b_id) = human("mark-order-b");
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Give(batch(vec![item(&a), item(&b)], None)),
    ]);
    let model = Scripted::new(vec![
        Reply::call("noop"),
        Reply::call("noop"),
        Reply::text("done"),
    ]);
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;

    let log = steps(&rig.record);
    let add_a = index_of(&log, &Step::AddEntry(a_id));
    let mark_a = index_of(&log, &Step::Mark(a_id));
    let add_b = index_of(&log, &Step::AddEntry(b_id));
    let mark_b = index_of(&log, &Step::Mark(b_id));
    assert!(add_a < mark_a);
    assert!(mark_a < add_b);
    assert!(add_b < mark_b);
}

#[tokio::test(start_paused = true)]
async fn given_items_stored_in_a_new_input_turn_when_the_drain_ends_then_each_mark_follows_the_hand_over_of_the_turn()
 {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let (a, a_id) = human("turn-mark-a");
    let (b, b_id) = human("turn-mark-b");
    rig.inbox.deliver_entry(rig.agent, a);
    rig.inbox.deliver_entry(rig.agent, b);
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;

    let log = steps(&rig.record);
    let added = index_of(&log, &Step::AddTurn(vec![a_id, b_id]));
    assert!(added < index_of(&log, &Step::Mark(a_id)));
    assert!(added < index_of(&log, &Step::Mark(b_id)));
    assert!(rig.inbox.is_read(rig.agent, a_id));
    assert!(rig.inbox.is_read(rig.agent, b_id));
}

#[tokio::test(start_paused = true)]
async fn given_slow_marks_at_start_when_the_agent_runs_then_the_first_step_waits_for_every_mark() {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let (a, _) = human("slow-mark-a");
    let (b, _) = human("slow-mark-b");
    rig.inbox.deliver_entry(rig.agent, a);
    rig.inbox.deliver_entry(rig.agent, b);
    spy.plan_marks(vec![
        Act::Sleep(Duration::from_secs(10)),
        Act::Sleep(Duration::from_secs(10)),
    ]);
    let model = Scripted::new(vec![Reply::text("done")]);
    let stamps = stamp_calls(&model);
    let node = spied(&rig, &spy, &model);
    let began = Instant::now();

    Driver::result(&node).await;

    let first_step = stamps.lock().unwrap()[0];
    assert!(first_step - began >= Duration::from_secs(20));
}

#[tokio::test(start_paused = true)]
async fn given_a_slow_mark_after_a_tool_run_when_the_loop_goes_on_then_the_next_step_waits_for_the_mark()
 {
    let rig = rig_with_noop();
    let spy = Spy::new(&rig);
    let (a, _) = human("slow-mark-in-the-loop");
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Give(batch(vec![item(&a)], None)),
    ]);
    spy.plan_marks(vec![Act::Sleep(Duration::from_secs(10))]);
    let model = Scripted::new(vec![
        Reply::call("noop"),
        Reply::call("noop"),
        Reply::text("done"),
    ]);
    let stamps = stamp_calls(&model);
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;

    let stamps = stamps.lock().unwrap();
    assert!(stamps[1] - stamps[0] >= Duration::from_secs(10));
}

#[tokio::test(start_paused = true)]
async fn given_a_thread_access_that_refuses_the_turn_when_the_drain_stores_then_the_item_is_not_marked()
 {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let (a, a_id) = human("never-handed-over");
    rig.inbox.deliver_entry(rig.agent, a);
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = through_access(
        &rig,
        FailingAdds::input_turn(&rig.access, 1),
        spy.clone(),
        &model,
    );

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("refused the turn"), "{cause}");
    assert!(!rig.inbox.is_read(rig.agent, a_id));
    assert!(spy.marked().is_empty());
    assert!(!names(&rig.record).contains(&"inbox_mark_read"));
}

#[tokio::test(start_paused = true)]
async fn given_a_thread_access_that_refuses_the_entry_when_the_boundary_stores_then_the_item_is_not_marked()
 {
    let rig = rig_with_noop();
    let spy = Spy::new(&rig);
    let (a, a_id) = human("never-handed-over-in-a-turn");
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Give(batch(vec![item(&a)], None)),
    ]);
    let model = Scripted::new(vec![
        Reply::call("noop"),
        Reply::call("noop"),
        Reply::text("done"),
    ]);
    let node = through_access(
        &rig,
        FailingAdds::entry(&rig.access, 1),
        spy.clone(),
        &model,
    );

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("refused the entry"), "{cause}");
    assert_eq!(spy.times_marked(a_id), 0);
    assert!(!names(&rig.record).contains(&"inbox_mark_read"));
}

#[tokio::test(start_paused = true)]
async fn given_a_mark_that_fails_when_the_agent_runs_then_it_is_no_fault_and_the_item_is_marked_by_a_later_skip()
 {
    let rig = rig_with_noop();
    let spy = Spy::new(&rig);
    let (a, a_id) = human("mark-fails-once");
    rig.inbox.deliver_entry(rig.agent, a);
    spy.plan_marks(vec![Act::Fault]);
    let model = Scripted::new(vec![
        Reply::call("noop"),
        Reply::call("noop"),
        Reply::text("done"),
    ]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    let conversation = rig.conversation();
    assert_eq!(count_of(&conversation, a_id), 1);
    let log = steps(&rig.record);
    let marks = log.iter().filter(|step| **step == Step::Mark(a_id)).count();
    assert_eq!(marks, 1);
    assert_eq!(spy.times_marked(a_id), 1);
    assert!(rig.inbox.is_read(rig.agent, a_id));
    assert_eq!(
        log.iter()
            .filter(|step| matches!(step, Step::AddTurn(ids) if ids.contains(&a_id)))
            .count(),
        1
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_mark_that_is_refused_when_the_agent_runs_then_it_is_no_fault() {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let (a, a_id) = human("mark-refused");
    rig.inbox.deliver_entry(rig.agent, a);
    spy.plan_marks(vec![Act::Refuse("not now".to_owned())]);
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    assert_eq!(count_of(&rig.conversation(), a_id), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_mark_in_flight_when_the_instance_is_cancelled_then_the_mark_is_dropped() {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let (a, _) = human("mark-hangs");
    rig.inbox.deliver_entry(rig.agent, a);
    spy.plan_marks(vec![Act::Hang]);
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = spied(&rig, &spy, &model);
    let mut driver = Driver::start(&node).await;
    driver.give();
    tokio::time::sleep(Duration::from_millis(100)).await;

    driver.cancel.handle().trigger();
    let event = driver.event().await;
    settle().await;

    assert!(matches!(event, Event::Cancelled));
    assert_eq!(spy.dropped_marks(), 1);
    assert_eq!(model.calls(), 0);
}
