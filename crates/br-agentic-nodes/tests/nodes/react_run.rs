use std::time::Duration;

use br_agentic_contract::{FileSystemMode, Session, TurnTarget, WorkspaceFile};
use br_graph_contract::Event;
use br_llm_messages::{
    ClosureReason, ContentBlock, ModelResponse, OpaqueJson, ResponseEnding, ResponseMetadata,
    RuntimeToolCall, ToolName,
};

use crate::common::*;

fn file(path: &str, content: &str) -> WorkspaceFile {
    WorkspaceFile {
        path: path.to_owned(),
        content: content.as_bytes().to_vec(),
    }
}

#[tokio::test(start_paused = true)]
async fn given_an_open_turn_of_the_agent_when_the_instance_starts_then_it_closes_it_as_interrupted_before_its_first_step()
 {
    let rig = Rig::new();
    let agent = rig.agent("agent").await;
    rig.start_as(agent, "go").await;
    let access = rig.hub.thread_access();
    let mut session = Session::new(&access, rig.thread, agent);
    let target = TurnTarget::Ai {
        id: turn_id(),
        author: author("agent"),
    };
    let entry = entry_id();
    session.open_response(target.clone(), entry).await.unwrap();
    let response = ModelResponse::new(
        entry,
        vec![ContentBlock::RuntimeToolCall(
            RuntimeToolCall::new(
                block_id(),
                ToolName::new("search").unwrap(),
                OpaqueJson::parse("{}").unwrap(),
            )
            .unwrap(),
        )],
        ResponseEnding::tool_use(),
        ResponseMetadata::new(),
    )
    .unwrap();
    session.complete_response(target, response).await.unwrap();
    let model = Scripted::new(vec![Reply::text("fresh")]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "fresh");
    let conversation = conversation(&rig.hub, &rig.thread);
    let turns = ai_turns(&conversation);
    assert_eq!(turns.len(), 2);
    assert!(matches!(
        closure(turns[0]),
        Some(ClosureReason::Interrupted { .. })
    ));
    assert!(matches!(
        closure(turns[1]),
        Some(ClosureReason::Completed { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn given_many_steps_when_the_instance_runs_then_the_prompt_port_is_called_once() {
    let (search, _) = echo("search");
    let rig = Rig::new().with_tools(vec![search]);
    rig.start_with("go").await;
    let model = Scripted::new(vec![
        Reply::call("search"),
        Reply::call("search"),
        Reply::text("done"),
    ]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert_eq!(rig.prompts.asked("agent"), 1);
}

#[tokio::test(start_paused = true)]
async fn given_two_inputs_when_the_instance_runs_then_it_uses_the_first_only_and_gives_one_result()
{
    let rig = Rig::new();
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::text("done"), Reply::text("never")]);
    let node = rig.react(&[&model]);
    let mut driver = Driver::start(&node).await;
    driver.give();
    driver.give();

    let event = driver.event().await;

    assert!(matches!(event, Event::Result(_)));
    assert_eq!(model.calls(), 1);
    assert_eq!(rig.prompts.asked("agent"), 1);
    assert_eq!(rig.outcome.outcomes().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_during_a_model_call_when_it_is_triggered_then_the_open_response_is_discarded_and_cancelled_is_sent()
 {
    let rig = Rig::new().with_timeouts(Duration::from_secs(600), Duration::from_secs(600));
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::Hang]);
    let node = rig.react(&[&model]);
    let mut driver = Driver::start(&node).await;
    driver.give();
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(model.calls(), 1);

    driver.cancel.handle().trigger();
    let event = driver.event().await;

    assert!(matches!(event, Event::Cancelled));
    assert!(!rig.hub.is_open(model.requests()[0].reply_entry()));
    let conversation = conversation(&rig.hub, &rig.thread);
    assert!(ai_turns(&conversation).is_empty());
    assert!(rig.outcome.outcomes().is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_during_a_tool_round_when_it_is_triggered_then_the_turn_is_closed_as_cancelled_and_what_was_stored_stays()
 {
    let (hanging, probe) = tool("hanging", Behavior::Hang);
    let rig = Rig::new()
        .with_tools(vec![hanging])
        .with_timeouts(Duration::from_secs(600), Duration::from_secs(600))
        .with_files(FileSystemMode::Always, vec![file("a.txt", "a")]);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::call("hanging")]);
    let node = rig.react(&[&model]);
    let mut driver = Driver::start(&node).await;
    driver.give();
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(*probe.starts.lock().unwrap(), 1);

    driver.cancel.handle().trigger();
    let event = driver.event().await;

    assert!(matches!(event, Event::Cancelled));
    let conversation = conversation(&rig.hub, &rig.thread);
    let turn = ai_turns(&conversation)[0];
    assert!(matches!(
        closure(turn),
        Some(ClosureReason::Cancelled { .. })
    ));
    assert_eq!(responses(turn).len(), 1);
    assert!(results(turn).is_empty());
    assert_eq!(rig.workspace.saves(), 0);
    assert!(rig.outcome.outcomes().is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_failure_when_the_instance_stops_then_no_file_is_saved_and_no_outcome_is_given() {
    let rig = Rig::new().with_files(FileSystemMode::Always, vec![file("a.txt", "a")]);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::Fail(
        br_llm_clients_core::ClientError::Transport,
    )]);
    let node = rig.react(&[&model]);

    let (_, event) = Driver::run(&node).await;

    assert!(matches!(event, Event::Failed(_)));
    assert_eq!(rig.workspace.saves(), 0);
    assert!(rig.outcome.outcomes().is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_the_always_mode_when_the_instance_ends_then_the_files_are_saved() {
    let rig = Rig::new().with_files(FileSystemMode::Always, vec![file("a.txt", "a")]);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert_eq!((rig.workspace.loads(), rig.workspace.saves()), (1, 1));
    assert_eq!(rig.workspace.files(), vec![file("a.txt", "a")]);
}

#[tokio::test(start_paused = true)]
async fn given_the_always_mode_with_no_file_when_the_instance_ends_then_an_empty_file_system_is_saved()
 {
    let rig = Rig::new().with_files(FileSystemMode::Always, Vec::new());
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert_eq!(rig.workspace.saves(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_the_might_mode_with_no_file_when_the_instance_ends_then_nothing_is_saved() {
    let rig = Rig::new().with_files(FileSystemMode::Might, Vec::new());
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert_eq!((rig.workspace.loads(), rig.workspace.saves()), (1, 0));
}

#[tokio::test(start_paused = true)]
async fn given_the_might_mode_with_files_when_the_instance_ends_then_they_are_saved() {
    let rig = Rig::new().with_files(FileSystemMode::Might, vec![file("a.txt", "a")]);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert_eq!(rig.workspace.saves(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_the_never_mode_when_the_instance_ends_then_nothing_is_loaded_or_saved() {
    let rig = Rig::new().with_files(FileSystemMode::Never, vec![file("a.txt", "a")]);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert_eq!((rig.workspace.loads(), rig.workspace.saves()), (0, 0));
}

#[tokio::test(start_paused = true)]
async fn given_a_path_outside_the_sandbox_when_the_files_are_loaded_then_the_instance_fails_before_any_step()
 {
    let rig = Rig::new().with_files(FileSystemMode::Always, vec![file("../escape.txt", "x")]);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::text("never")]);
    let node = rig.react(&[&model]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("escape.txt"), "{cause}");
    assert_eq!(model.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_step_limit_stop_when_the_instance_ends_then_the_files_are_saved_and_a_result_is_sent()
 {
    let (search, _) = echo("search");
    let rig = Rig::new()
        .with_tools(vec![search])
        .with_step_limit(1)
        .with_files(FileSystemMode::Always, vec![file("a.txt", "a")]);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::call("search")]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert!(update.step_limit);
    assert_eq!(rig.workspace.saves(), 1);
}
