use std::sync::Arc;
use std::time::Duration;

use br_agentic_contract::memory::Call;

use crate::access_spy::{AccessSpy, Op};
use crate::common::*;
use crate::kit::{Parts, react_list, react_over};

fn spied(rig: &Rig) -> (Parts, Arc<AccessSpy>) {
    let spy = Arc::new(AccessSpy::over(rig.access.clone()));
    (Parts::of(rig).with_access(spy.clone()), spy)
}

fn forgetful(rig: &Rig) -> (Parts, Arc<AccessSpy>) {
    let spy = Arc::new(AccessSpy::forgetful(rig.conversation(), rig.access.clone()));
    (Parts::of(rig).with_access(spy.clone()), spy)
}

#[tokio::test(start_paused = true)]
async fn given_a_run_with_several_tool_rounds_when_it_ends_then_the_thread_was_read_once_before_any_change()
 {
    let (tool_a, _) = echo("a");
    let rig = Rig::new().with_tools(vec![tool_a]);
    let model = Scripted::new(vec![
        Reply::call("a"),
        Reply::call("a"),
        Reply::call("a"),
        Reply::text("done"),
    ]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.steps, Some(4));
    let seen = names(&rig.record);
    assert_eq!(seen.iter().filter(|name| **name == "read").count(), 1);
    let read = seen.iter().position(|name| *name == "read").unwrap();
    let first_change = seen
        .iter()
        .position(|name| matches!(*name, "open_response" | "add_input_turn" | "add_entry"))
        .unwrap();
    assert!(read < first_change);
}

#[tokio::test(start_paused = true)]
async fn given_a_thread_access_that_keeps_nothing_when_the_agent_runs_then_the_result_is_the_same_as_with_a_faithful_one()
 {
    let (tool_a, _) = echo("a");
    let faithful_rig = Rig::new().with_tools(vec![tool_a.clone()]);
    let faithful_model = Scripted::new(vec![Reply::call("a"), Reply::text("final")]);
    let faithful = Driver::result(&faithful_rig.react(&[&faithful_model])).await;

    let rig = Rig::new().with_tools(vec![tool_a]);
    let (parts, spy) = forgetful(&rig);
    let model = Scripted::new(vec![Reply::call("a"), Reply::text("final")]);
    let node = react_over(&rig, parts, &[&model]);

    let forgetting = Driver::result(&node).await;

    assert_eq!(forgetting, faithful);
    assert_eq!(forgetting.text, "final");
    assert_eq!(forgetting.steps, Some(2));
    let second = history_text(&model.requests()[1]);
    assert!(second.contains("a done"));
    assert!(second.contains("start"));
    assert_eq!(spy.count(Op::Read), 1);
    assert!(rig.conversation().turns().len() == 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_thread_access_that_keeps_nothing_when_the_agent_runs_then_the_node_never_reads_back_what_it_wrote()
 {
    let (tool_a, _) = echo("a");
    let rig = Rig::new().with_tools(vec![tool_a]);
    let (parts, spy) = forgetful(&rig);
    let model = Scripted::new(vec![Reply::call("a"), Reply::call("a"), Reply::text("end")]);
    let node = react_over(&rig, parts, &[&model]);

    Driver::result(&node).await;

    assert_eq!(spy.count(Op::Read), 1);
    let third = history_text(&model.requests()[2]);
    assert_eq!(third.matches("a done").count(), 2);
}

#[tokio::test(start_paused = true)]
async fn given_a_hand_over_that_returns_at_once_while_storage_is_slow_when_the_agent_runs_then_the_node_is_not_slowed()
 {
    let (tool_a, _) = echo("a");
    let rig = Rig::new().with_tools(vec![tool_a]);
    let spy = Arc::new(
        AccessSpy::forgetful(rig.conversation(), rig.access.clone())
            .with_slow_storage(Duration::from_secs(3600)),
    );
    let model = Scripted::new(vec![Reply::call("a"), Reply::text("end")]);
    let node = react_over(&rig, Parts::of(&rig).with_access(spy.clone()), &[&model]);
    let before = tokio::time::Instant::now();

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "end");
    assert!(before.elapsed() < Duration::from_secs(60));
    assert!(spy.count(Op::CompleteResponse) == 2);
}

#[tokio::test(start_paused = true)]
async fn given_a_conversation_that_is_empty_when_the_inbox_gives_a_request_then_the_empty_conversation_is_valid_and_the_run_ends()
 {
    let rig = Rig::empty().with_inbox();
    rig.inbox
        .deliver_entry(rig.agent, inbox_entry("the first words"));
    let model = Scripted::new(vec![Reply::text("answer")]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "answer");
    assert!(history_text(&model.requests()[0]).contains("the first words"));
    assert_eq!(input_turns(&rig.conversation()).len(), 1);
    assert_eq!(ai_turns(&rig.conversation()).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_slow_thread_access_when_two_tools_run_then_each_hand_over_is_awaited_before_the_next_and_in_order()
 {
    let (tool_a, _) = echo("a");
    let (tool_b, _) = echo("b");
    let rig = Rig::new().with_tools(vec![tool_a, tool_b]);
    let delay = Duration::from_millis(10);
    let spy = Arc::new(AccessSpy::over(rig.access.clone()).slow(delay));
    let model = Scripted::new(vec![Reply::calls(&["a", "b"]), Reply::text("end")]);
    let node = react_over(&rig, Parts::of(&rig).with_access(spy.clone()), &[&model]);

    Driver::result(&node).await;

    assert_eq!(
        spy.ops(),
        vec![
            Op::Read,
            Op::OpenResponse,
            Op::CompleteResponse,
            Op::OpenResult,
            Op::OpenResult,
            Op::CompleteResult,
            Op::CompleteResult,
            Op::OpenResponse,
            Op::CompleteResponse,
            Op::CloseTurn,
        ]
    );
    assert_eq!(spy.max_in_flight(), 1);
    let log = spy.log();
    for pair in log.windows(2) {
        assert!(pair[1].started - pair[0].started >= delay);
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_streamed_response_when_it_ends_then_the_open_and_the_completion_are_the_only_operations_for_it()
 {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("hello")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert_eq!(
        names(&rig.record)
            .into_iter()
            .filter(|name| *name != "thread_port" && *name != "agent_port" && *name != "tools_port")
            .collect::<Vec<_>>(),
        vec!["read", "open_response", "complete_response", "close_turn"]
    );
    let calls = rig.record.calls();
    let opened = calls.iter().find_map(|call| match call {
        Call::OpenResponse { entry, .. } => Some(*entry),
        _ => None,
    });
    let completed = calls.iter().find_map(|call| match call {
        Call::CompleteResponse { response, .. } => Some(response.id()),
        _ => None,
    });
    assert_eq!(opened, completed);
    assert!(opened.is_some());
}

#[tokio::test(start_paused = true)]
async fn given_a_streamed_result_when_the_tool_ends_then_the_open_and_the_completion_are_the_only_operations_for_it()
 {
    let (tool_a, _) = echo("a");
    let rig = Rig::new().with_tools(vec![tool_a]);
    let model = Scripted::new(vec![Reply::call("a"), Reply::text("end")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let seen = names(&rig.record);
    assert_eq!(
        seen.iter().filter(|name| **name == "open_result").count(),
        1
    );
    assert_eq!(
        seen.iter()
            .filter(|name| **name == "complete_result")
            .count(),
        1
    );
    assert_eq!(seen.iter().filter(|name| **name == "add_entry").count(), 0);
    let open = seen.iter().position(|name| *name == "open_result").unwrap();
    let complete = seen
        .iter()
        .position(|name| *name == "complete_result")
        .unwrap();
    assert!(open < complete);
}

#[tokio::test(start_paused = true)]
async fn given_a_discarded_response_when_the_next_client_answers_then_the_copy_and_the_thread_hold_only_the_kept_response()
 {
    let rig = Rig::new();
    let silent = Scripted::new(vec![Reply::text("lost words")]);
    let kept = Scripted::new(vec![Reply::text("kept words")]);
    let first = thinking_client(&silent);
    let second = client(&kept);
    let fallback = br_agentic_nodes::FallbackList::new(vec![first, second]).unwrap();
    let node = react_list(&rig, Parts::of(&rig), fallback);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "kept words");
    let conversation = rig.conversation();
    let turns = ai_turns(&conversation);
    assert_eq!(turns.len(), 1);
    assert_eq!(responses(turns[0]), vec!["kept words".to_owned()]);
    assert!(!format!("{conversation:?}").contains("lost words"));
    assert_eq!(
        history_text(&silent.requests()[0]),
        history_text(&kept.requests()[0])
    );
    let seen = names(&rig.record);
    let tail: Vec<_> = seen
        .into_iter()
        .filter(|name| name.contains("response"))
        .collect();
    assert_eq!(
        tail,
        vec![
            "open_response",
            "discard_response",
            "open_response",
            "complete_response"
        ]
    );
}
