use std::sync::Arc;
use std::time::Duration;

use br_agentic_contract::{FileSystemMode, WorkspaceFile};
use br_agentic_nodes::{CallFailure, Fault};
use br_graph_contract::Event;
use br_llm_clients_core::ClientError;
use br_llm_messages::ClosureReason;

use crate::access_spy::{AccessSpy, Op};
use crate::common::*;
use crate::drive::*;
use crate::hanging::ProbeInbox;
use crate::kit::{Parts, react_list, react_over, react_spec};
use crate::ports_kit::*;

fn file(path: &str) -> WorkspaceFile {
    WorkspaceFile {
        path: path.to_owned(),
        content: b"kept".to_vec(),
    }
}

fn failed_detail(reason: &ClosureReason) -> Option<&str> {
    match reason {
        ClosureReason::Failed { .. } => reason.detail(),
        _ => None,
    }
}

#[tokio::test(start_paused = true)]
async fn given_an_outcome_port_that_fails_when_the_agent_ends_then_the_instance_fails_without_a_retry_and_saves_no_files()
 {
    let rig = Rig::new().with_files(FileSystemMode::Always, vec![file("a.txt")]);
    let model = Scripted::new(vec![Reply::text("done")]);
    let trip = Trip::failing();
    let spy = Arc::new(AccessSpy::over(rig.access.clone()));
    let node = react_spec(
        &rig,
        Parts::of(&rig).with_access(spy.clone()),
        list(&[&model]),
        Arc::new(FailingOutcome(trip.clone())),
        None,
    );

    let (driver, event) = Driver::run(&node).await;

    assert!(is_failed(&event), "{event:?}");
    assert_eq!(trip.calls(), 1);
    assert!(!names(&rig.record).contains(&"workspace_save"));
    assert_eq!(spy.closures(rig.agent).len(), 1);
    assert!(matches!(
        spy.closures(rig.agent)[0],
        ClosureReason::Completed { .. }
    ));
    assert!(input_is_closed(&driver));
}

#[tokio::test(start_paused = true)]
async fn given_a_workspace_save_that_fails_when_the_agent_ends_then_the_instance_fails_without_a_retry()
 {
    let rig = Rig::new().with_files(FileSystemMode::Always, vec![file("a.txt")]);
    let model = Scripted::new(vec![Reply::text("done")]);
    let flaky = FlakyWorkspace::new(rig.workspace.clone(), false, true);
    let mut parts = Parts::of(&rig);
    parts.workspace = flaky.clone();
    let node = react_over(&rig, parts, &[&model]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("the save failed"), "{cause}");
    assert_eq!(flaky.saves(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_single_client_that_fails_when_the_agent_runs_then_the_instance_fails_after_one_call_with_no_turn_to_close()
 {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::Fail(ClientError::Transport)]);
    let node = rig.react(&[&model]);

    let (driver, event) = Driver::run(&node).await;

    assert!(is_failed(&event), "{event:?}");
    assert_eq!(model.calls(), 1);
    assert!(ai_turns(&rig.conversation()).is_empty());
    let seen = names(&rig.record);
    assert!(!seen.contains(&"close_turn"));
    assert!(seen.contains(&"discard_response"));
    assert_eq!(rig.outcome.calls(), 0);
    assert!(input_is_closed(&driver));
}

#[tokio::test(start_paused = true)]
async fn given_two_clients_that_both_fail_when_the_agent_runs_then_each_is_called_once_and_the_instance_fails()
 {
    let rig = Rig::new();
    let first = Scripted::new(vec![Reply::Fail(ClientError::Transport)]);
    let second = Scripted::new(vec![Reply::Fail(ClientError::StreamTruncated)]);
    let node = rig.react(&[&first, &second]);

    let event = Driver::run(&node).await.1;

    assert!(is_failed(&event), "{event:?}");
    assert_eq!(first.calls(), 1);
    assert_eq!(second.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_that_faults_next_to_a_tool_that_hangs_when_the_agent_runs_then_everything_stops_and_the_turn_fails_with_the_reason()
 {
    let (broken, broken_probe) = tool("broken", Behavior::Fault);
    let (stuck, stuck_probe) = tool("stuck", Behavior::Hang);
    let rig = Rig::new()
        .with_tools(vec![broken, stuck])
        .with_files(FileSystemMode::Always, vec![file("a.txt")]);
    let spy = Arc::new(AccessSpy::over(rig.access.clone()));
    let model = Scripted::new(vec![Reply::calls(&["broken", "stuck"])]);
    let node = react_over(&rig, Parts::of(&rig).with_access(spy.clone()), &[&model]);

    let (driver, event) = Driver::run(&node).await;

    assert!(is_failed(&event), "{event:?}");
    assert_eq!(*broken_probe.starts.lock().unwrap(), 1);
    assert_eq!(*stuck_probe.starts.lock().unwrap(), 1);
    assert_eq!(spy.count(Op::DiscardResult), 2);
    assert_eq!(spy.count(Op::CompleteResult), 0);
    let closures = spy.closures(rig.agent);
    assert_eq!(closures.len(), 1);
    let detail = failed_detail(&closures[0]).expect("a Failed closure");
    assert!(detail.contains("the tool broke"), "{detail}");
    let conversation = rig.conversation();
    assert_eq!(ai_turns(&conversation).len(), 1);
    assert_eq!(responses(ai_turns(&conversation)[0]).len(), 1);
    assert!(!names(&rig.record).contains(&"workspace_save"));
    assert_eq!(rig.outcome.calls(), 0);
    assert!(input_is_closed(&driver));
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_that_faults_when_the_agent_runs_then_the_fault_reaches_the_event_with_its_source_chain_and_the_tool_runs_once()
 {
    let (broken, probe) = tool("broken", Behavior::Fault);
    let rig = Rig::new().with_tools(vec![broken]);
    let model = Scripted::new(vec![Reply::call("broken"), Reply::text("never")]);
    let node = rig.react(&[&model]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("the tool broke"), "{cause}");
    assert_eq!(*probe.starts.lock().unwrap(), 1);
    assert_eq!(model.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_task_that_panics_when_the_agent_runs_then_the_instance_fails_instead_of_unwinding()
 {
    let (broken, _) = tool("broken", Behavior::Panic);
    let rig = Rig::new().with_tools(vec![broken]);
    let model = Scripted::new(vec![Reply::call("broken"), Reply::text("never")]);
    let node = rig.react(&[&model]);

    let (driver, event) = Driver::run(&node).await;

    assert!(is_failed(&event), "{event:?}");
    assert_eq!(model.calls(), 1);
    let turns = closures_of(&rig.conversation());
    assert!(matches!(&turns[0], Some(ClosureReason::Failed { .. })));
    assert!(input_is_closed(&driver));
}

#[tokio::test(start_paused = true)]
async fn given_a_model_call_that_fails_after_a_tool_round_when_the_agent_runs_then_the_open_turn_closes_failed_with_the_reason_as_detail()
 {
    let (tool_a, _) = echo("a");
    let rig = Rig::new().with_tools(vec![tool_a]);
    let model = Scripted::new(vec![Reply::call("a"), Reply::Fail(ClientError::Transport)]);
    let node = rig.react(&[&model]);
    let expected = Fault::Model(CallFailure::Client(ClientError::Transport)).detail();

    let event = Driver::run(&node).await.1;

    assert!(is_failed(&event), "{event:?}");
    let conversation = rig.conversation();
    let turns = ai_turns(&conversation);
    assert_eq!(turns.len(), 1);
    let reason = closure(turns[0]).expect("a closure");
    assert_eq!(failed_detail(&reason), Some(expected.as_str()));
    assert!(names(&rig.record).contains(&"discard_response"));
    assert_eq!(responses(turns[0]).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_closure_and_discards_that_fail_when_the_instance_fails_then_those_errors_are_ignored_and_the_original_fault_is_reported()
 {
    let (broken, _) = tool("broken", Behavior::Fault);
    let rig = Rig::new().with_tools(vec![broken]);
    let spy = Arc::new(
        AccessSpy::over(rig.access.clone())
            .failing(Op::CloseTurn)
            .failing(Op::DiscardResult)
            .failing(Op::DiscardResponse),
    );
    let model = Scripted::new(vec![Reply::call("broken")]);
    let node = react_over(&rig, Parts::of(&rig).with_access(spy.clone()), &[&model]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("the tool broke"), "{cause}");
    assert!(!cause.contains("CloseTurn failed"));
    assert!(!cause.contains("DiscardResult failed"));
    assert!(spy.count(Op::CloseTurn) >= 1);
    assert!(spy.count(Op::DiscardResult) >= 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_running_inbox_read_when_the_instance_fails_then_the_read_is_dropped() {
    let (quick, _) = echo("quick");
    let (broken, _) = tool("broken", Behavior::Fault);
    let rig = Rig::new().with_inbox().with_tools(vec![quick, broken]);
    let inbox = ProbeInbox::hanging_from(rig.inbox.clone(), 1);
    let mut parts = Parts::of(&rig);
    parts.inbox = Some(inbox.clone());
    let model = Scripted::new(vec![Reply::call("quick"), Reply::call("broken")]);
    let node = react_over(&rig, parts, &[&model]);

    let event = Driver::run(&node).await.1;

    assert!(is_failed(&event), "{event:?}");
    tokio::time::timeout(Duration::from_secs(1), inbox.started.notified())
        .await
        .expect("the read had started");
    tokio::time::sleep(Duration::from_millis(1)).await;
    assert_eq!(inbox.dropped(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_run_with_tool_rounds_when_it_ends_then_no_object_is_left_open_for_the_final_discard()
 {
    let (tool_a, _) = echo("a");
    let rig = Rig::new().with_tools(vec![tool_a]);
    let spy = Arc::new(
        AccessSpy::over(rig.access.clone())
            .failing(Op::DiscardResult)
            .failing(Op::DiscardResponse),
    );
    let model = Scripted::new(vec![Reply::calls(&["a", "a"]), Reply::text("end")]);
    let node = react_over(&rig, Parts::of(&rig).with_access(spy.clone()), &[&model]);

    let event = Driver::run(&node).await.1;

    assert!(matches!(event, Event::Result(_)), "{event:?}");
    assert_eq!(spy.count(Op::DiscardResult), 0);
    assert_eq!(spy.count(Op::DiscardResponse), 0);
    let opens = spy.count(Op::OpenResponse) + spy.count(Op::OpenResult);
    let completions = spy.count(Op::CompleteResponse) + spy.count(Op::CompleteResult);
    assert_eq!(opens, completions);
}

#[tokio::test(start_paused = true)]
async fn given_a_fallback_list_that_is_used_up_when_the_agent_runs_then_the_last_error_is_the_fault()
 {
    let rig = Rig::new();
    let first = Scripted::new(vec![Reply::Fail(ClientError::Transport)]);
    let second = Scripted::new(vec![Reply::Fail(ClientError::StreamTruncated)]);
    let node = react_list(&rig, Parts::of(&rig), list(&[&first, &second]));
    let expected = Fault::Model(CallFailure::Client(ClientError::StreamTruncated)).detail();

    let cause = Driver::failure(&node).await;

    assert!(
        cause.contains(expected.split(": ").last().unwrap()),
        "{cause}"
    );
}
