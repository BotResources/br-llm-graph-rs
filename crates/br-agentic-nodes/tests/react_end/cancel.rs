use std::sync::Arc;
use std::time::Duration;

use br_agentic_contract::{FileSystemMode, WorkspaceFile};
use br_graph_contract::Event;
use br_llm_messages::ClosureReason;

use crate::access_spy::{AccessSpy, Op};
use crate::common::*;
use crate::drive::*;
use crate::hanging::ProbeInbox;
use crate::kit::{Parts, react_over};
use crate::ports_kit::*;

fn file() -> WorkspaceFile {
    WorkspaceFile {
        path: "a.txt".to_owned(),
        content: b"kept".to_vec(),
    }
}

async fn cancel_when(driver: &mut Driver, ready: impl FnMut() -> bool) -> Event<Doc> {
    until(ready).await;
    driver.cancel.handle().trigger();
    driver.event().await
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_before_any_input_when_the_instance_waits_then_it_sends_cancelled_without_touching_a_port()
 {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("never")]);
    let node = rig.react(&[&model]);
    let mut driver = Driver::start(&node).await;

    driver.cancel.handle().trigger();
    let event = driver.event().await;

    assert!(is_cancelled(&event), "{event:?}");
    assert!(rig.record.calls().is_empty());
    assert!(input_is_closed(&driver));
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_during_a_model_call_after_a_tool_round_when_the_agent_runs_then_everything_stops_and_the_turn_closes_cancelled()
 {
    let (tool_a, _) = echo("a");
    let rig = Rig::new()
        .with_tools(vec![tool_a])
        .with_files(FileSystemMode::Always, vec![file()]);
    let spy = Arc::new(AccessSpy::over(rig.access.clone()));
    let model = Scripted::new(vec![Reply::call("a"), Reply::Hang]);
    let node = react_over(&rig, Parts::of(&rig).with_access(spy.clone()), &[&model]);
    let mut driver = Driver::start(&node).await;
    driver.give();

    let event = cancel_when(&mut driver, || model.calls() == 2).await;

    assert!(is_cancelled(&event), "{event:?}");
    assert_eq!(spy.count(Op::OpenResponse), 2);
    assert_eq!(spy.count(Op::DiscardResponse), 1);
    let closures = spy.closures(rig.agent);
    assert_eq!(closures.len(), 1);
    assert!(matches!(closures[0], ClosureReason::Cancelled { .. }));
    let conversation = rig.conversation();
    let turns = ai_turns(&conversation);
    assert_eq!(turns.len(), 1);
    assert_eq!(responses(turns[0]).len(), 1);
    assert_eq!(texts_of_results(turns[0]).len(), 1);
    assert!(!names(&rig.record).contains(&"workspace_save"));
    assert_eq!(rig.outcome.calls(), 0);
    assert!(input_is_closed(&driver));
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_during_the_first_model_call_when_no_turn_exists_then_the_response_is_discarded_and_nothing_is_closed()
 {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::Hang]);
    let node = rig.react(&[&model]);
    let mut driver = Driver::start(&node).await;
    driver.give();

    let event = cancel_when(&mut driver, || model.calls() == 1).await;

    assert!(is_cancelled(&event), "{event:?}");
    let seen = names(&rig.record);
    assert!(seen.contains(&"open_response"));
    assert!(seen.contains(&"discard_response"));
    assert!(!seen.contains(&"close_turn"));
    assert!(ai_turns(&rig.conversation()).is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_during_a_tool_call_when_the_agent_runs_then_the_open_result_is_discarded_and_the_turn_closes_cancelled()
 {
    let (stuck, probe) = tool("stuck", Behavior::Hang);
    let rig = Rig::new()
        .with_tools(vec![stuck])
        .with_files(FileSystemMode::Always, vec![file()]);
    let spy = Arc::new(AccessSpy::over(rig.access.clone()));
    let model = Scripted::new(vec![Reply::call("stuck"), Reply::text("never")]);
    let node = react_over(&rig, Parts::of(&rig).with_access(spy.clone()), &[&model]);
    let mut driver = Driver::start(&node).await;
    driver.give();

    let event = cancel_when(&mut driver, || *probe.starts.lock().unwrap() == 1).await;

    assert!(is_cancelled(&event), "{event:?}");
    assert_eq!(model.calls(), 1);
    assert_eq!(spy.count(Op::DiscardResult), 1);
    assert_eq!(spy.count(Op::CompleteResult), 0);
    let closures = spy.closures(rig.agent);
    assert_eq!(closures.len(), 1);
    assert!(matches!(closures[0], ClosureReason::Cancelled { .. }));
    let conversation = rig.conversation();
    assert_eq!(responses(ai_turns(&conversation)[0]).len(), 1);
    assert!(!names(&rig.record).contains(&"workspace_save"));
    assert!(input_is_closed(&driver));
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_and_ports_whose_closure_and_discards_fail_when_the_agent_runs_then_those_errors_are_ignored()
 {
    let (stuck, probe) = tool("stuck", Behavior::Hang);
    let rig = Rig::new().with_tools(vec![stuck]);
    let spy = Arc::new(
        AccessSpy::over(rig.access.clone())
            .failing(Op::CloseTurn)
            .failing(Op::DiscardResult),
    );
    let model = Scripted::new(vec![Reply::call("stuck")]);
    let node = react_over(&rig, Parts::of(&rig).with_access(spy.clone()), &[&model]);
    let mut driver = Driver::start(&node).await;
    driver.give();

    let event = cancel_when(&mut driver, || *probe.starts.lock().unwrap() == 1).await;

    assert!(is_cancelled(&event), "{event:?}");
    assert!(spy.count(Op::CloseTurn) >= 1);
    assert!(spy.count(Op::DiscardResult) >= 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_during_a_pending_prompt_port_call_when_the_instance_starts_then_it_sends_cancelled_and_calls_no_model()
 {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("never")]);
    let trip = Trip::hanging();
    let mut parts = Parts::of(&rig);
    parts.prompt = Arc::new(FailingPrompt(trip.clone()));
    let node = react_over(&rig, parts, &[&model]);
    let mut driver = Driver::start(&node).await;
    driver.give();

    let event = cancel_when(&mut driver, || trip.calls() == 1).await;

    assert!(is_cancelled(&event), "{event:?}");
    assert_eq!(model.calls(), 0);
    assert!(input_is_closed(&driver));
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_during_a_pending_tools_port_call_when_the_instance_starts_then_it_sends_cancelled()
 {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("never")]);
    let trip = Trip::hanging();
    let mut parts = Parts::of(&rig);
    parts.tools = Arc::new(FailingToolsPort(trip.clone()));
    let node = react_over(&rig, parts, &[&model]);
    let mut driver = Driver::start(&node).await;
    driver.give();

    let event = cancel_when(&mut driver, || trip.calls() == 1).await;

    assert!(is_cancelled(&event), "{event:?}");
    assert_eq!(model.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_during_a_pending_thread_port_call_when_the_instance_starts_then_it_sends_cancelled()
 {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("never")]);
    let trip = Trip::hanging();
    let mut parts = Parts::of(&rig);
    parts.thread = Arc::new(FailingThreadPort(trip.clone()));
    let node = react_over(&rig, parts, &[&model]);
    let mut driver = Driver::start(&node).await;
    driver.give();

    let event = cancel_when(&mut driver, || trip.calls() == 1).await;

    assert!(is_cancelled(&event), "{event:?}");
    assert_eq!(model.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_during_a_pending_read_of_the_thread_access_when_the_instance_starts_then_it_sends_cancelled()
 {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("never")]);
    let spy = Arc::new(AccessSpy::over(rig.access.clone()).hanging(Op::Read));
    let node = react_over(&rig, Parts::of(&rig).with_access(spy.clone()), &[&model]);
    let mut driver = Driver::start(&node).await;
    driver.give();

    let event = cancel_when(&mut driver, || spy.count(Op::Read) == 1).await;

    assert!(is_cancelled(&event), "{event:?}");
    assert_eq!(model.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_during_a_pending_hand_over_when_the_agent_runs_then_the_open_response_is_discarded_and_the_instance_cancels()
 {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("done")]);
    let spy = Arc::new(AccessSpy::over(rig.access.clone()).hanging(Op::CompleteResponse));
    let node = react_over(&rig, Parts::of(&rig).with_access(spy.clone()), &[&model]);
    let mut driver = Driver::start(&node).await;
    driver.give();

    let event = cancel_when(&mut driver, || spy.count(Op::CompleteResponse) == 1).await;

    assert!(is_cancelled(&event), "{event:?}");
    assert_eq!(spy.count(Op::DiscardResponse), 1);
    assert_eq!(rig.outcome.calls(), 0);
    let closures = spy.closures(rig.agent);
    assert_eq!(closures.len(), 1);
    assert!(matches!(closures[0], ClosureReason::Cancelled { .. }));
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_during_the_inbox_read_of_start_when_the_agent_starts_then_it_sends_cancelled_before_any_model_call()
 {
    let rig = Rig::new().with_inbox();
    let model = Scripted::new(vec![Reply::text("never")]);
    let inbox = ProbeInbox::hanging_from(rig.inbox.clone(), 0);
    let mut parts = Parts::of(&rig);
    parts.inbox = Some(inbox.clone());
    let node = react_over(&rig, parts, &[&model]);
    let mut driver = Driver::start(&node).await;
    driver.give();
    tokio::time::timeout(Duration::from_secs(1), inbox.started.notified())
        .await
        .expect("the read had started");

    driver.cancel.handle().trigger();
    let event = driver.event().await;

    assert!(is_cancelled(&event), "{event:?}");
    assert_eq!(model.calls(), 0);
    assert!(input_is_closed(&driver));
    tokio::time::sleep(Duration::from_millis(1)).await;
    assert_eq!(inbox.dropped(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_running_inbox_read_when_the_instance_is_cancelled_then_the_read_is_dropped() {
    let (stuck, probe) = tool("stuck", Behavior::Hang);
    let rig = Rig::new().with_inbox().with_tools(vec![stuck]);
    let inbox = ProbeInbox::hanging_from(rig.inbox.clone(), 1);
    let mut parts = Parts::of(&rig);
    parts.inbox = Some(inbox.clone());
    let model = Scripted::new(vec![Reply::call("stuck")]);
    let node = react_over(&rig, parts, &[&model]);
    let mut driver = Driver::start(&node).await;
    driver.give();

    let event = cancel_when(&mut driver, || *probe.starts.lock().unwrap() == 1).await;

    assert!(is_cancelled(&event), "{event:?}");
    tokio::time::timeout(Duration::from_secs(1), inbox.started.notified())
        .await
        .expect("the read had started");
    tokio::time::sleep(Duration::from_millis(1)).await;
    assert_eq!(inbox.dropped(), 1);
}
