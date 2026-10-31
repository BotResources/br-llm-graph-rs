use std::sync::Arc;

use br_agentic_contract::FileSystemMode;
use br_agentic_nodes::{AuthorSource, ReactAgent};
use br_graph_contract::Event;

use crate::access_spy::{AccessSpy, Op};
use crate::common::*;
use crate::kit::{Parts, react_over, react_spec};
use crate::ports_kit::*;

async fn start_fault(rig: &Rig, node: ReactAgent<Doc>, model: &Arc<Scripted>, needle: &str) {
    let cause = Driver::failure(&node).await;

    assert!(cause.contains(needle), "{cause}");
    assert_eq!(model.calls(), 0);
    assert!(ai_turns(&rig.conversation()).is_empty());
    assert!(!names(&rig.record).contains(&"workspace_save"));
    assert!(!names(&rig.record).contains(&"open_response"));
}

#[tokio::test(start_paused = true)]
async fn given_a_thread_port_that_fails_when_the_instance_starts_then_it_fails_without_a_retry() {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("never")]);
    let trip = Trip::failing();
    let mut parts = Parts::of(&rig);
    parts.thread = Arc::new(FailingThreadPort(trip.clone()));

    start_fault(
        &rig,
        react_over(&rig, parts, &[&model]),
        &model,
        "the port failed",
    )
    .await;

    assert_eq!(trip.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_an_agent_port_that_fails_when_the_instance_starts_then_it_fails_without_a_retry() {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("never")]);
    let trip = Trip::failing();
    let mut parts = Parts::of(&rig);
    parts.agent = Arc::new(FailingAgentPort(trip.clone()));

    start_fault(
        &rig,
        react_over(&rig, parts, &[&model]),
        &model,
        "the port failed",
    )
    .await;

    assert_eq!(trip.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_read_of_the_thread_access_that_fails_when_the_instance_starts_then_it_fails_without_a_retry()
 {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("never")]);
    let spy = Arc::new(AccessSpy::over(rig.access.clone()).failing(Op::Read));
    let parts = Parts::of(&rig).with_access(spy.clone());

    start_fault(
        &rig,
        react_over(&rig, parts, &[&model]),
        &model,
        "Read failed",
    )
    .await;

    assert_eq!(spy.count(Op::Read), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_prompt_port_that_fails_when_the_instance_starts_then_it_fails_without_a_retry() {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("never")]);
    let trip = Trip::failing();
    let mut parts = Parts::of(&rig);
    parts.prompt = Arc::new(FailingPrompt(trip.clone()));

    start_fault(
        &rig,
        react_over(&rig, parts, &[&model]),
        &model,
        "the port failed",
    )
    .await;

    assert_eq!(trip.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_blank_prompt_when_the_instance_starts_then_it_fails_before_any_model_call() {
    let rig = Rig::new().with_prompts(Prompts::blank());
    let model = Scripted::new(vec![Reply::text("never")]);
    let node = rig.react(&[&model]);

    let cause = Driver::failure(&node).await;

    assert!(!cause.is_empty());
    assert_eq!(model.calls(), 0);
    assert!(!names(&rig.record).contains(&"open_response"));
    assert_eq!(rig.outcome.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_tools_port_that_fails_when_the_instance_starts_then_it_fails_without_a_retry() {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("never")]);
    let trip = Trip::failing();
    let mut parts = Parts::of(&rig);
    parts.tools = Arc::new(FailingToolsPort(trip.clone()));

    start_fault(
        &rig,
        react_over(&rig, parts, &[&model]),
        &model,
        "the port failed",
    )
    .await;

    assert_eq!(trip.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_an_author_port_that_fails_when_the_instance_starts_then_it_fails_without_a_retry() {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("never")]);
    let trip = Trip::failing();
    let author = AuthorSource::Port(Arc::new(FailingAuthor(trip.clone())));
    let node = react_spec(
        &rig,
        Parts::of(&rig),
        list(&[&model]),
        rig.outcome.clone(),
        Some(author),
    );

    start_fault(&rig, node, &model, "the port failed").await;

    assert_eq!(trip.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_an_author_name_that_the_conversation_refuses_when_the_instance_starts_then_it_fails_before_any_model_call()
 {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("never")]);
    let author = AuthorSource::Port(Arc::new(GivenAuthor(String::new())));
    let node = react_spec(
        &rig,
        Parts::of(&rig),
        list(&[&model]),
        rig.outcome.clone(),
        Some(author),
    );

    let cause = Driver::failure(&node).await;

    assert!(!cause.is_empty());
    assert_eq!(model.calls(), 0);
    assert!(!names(&rig.record).contains(&"open_response"));
}

async fn load_fault(mode: FileSystemMode) {
    let rig = Rig::new().with_files(mode, Vec::new());
    let model = Scripted::new(vec![Reply::text("never")]);
    let flaky = FlakyWorkspace::new(rig.workspace.clone(), true, false);
    let mut parts = Parts::of(&rig);
    parts.workspace = flaky.clone();

    start_fault(
        &rig,
        react_over(&rig, parts, &[&model]),
        &model,
        "the load failed",
    )
    .await;

    assert_eq!(flaky.loads(), 1);
    assert_eq!(flaky.saves(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_workspace_load_that_fails_and_the_mode_always_when_the_instance_starts_then_it_fails()
 {
    load_fault(FileSystemMode::Always).await;
}

#[tokio::test(start_paused = true)]
async fn given_a_workspace_load_that_fails_and_the_mode_might_when_the_instance_starts_then_it_fails()
 {
    load_fault(FileSystemMode::Might).await;
}

#[tokio::test(start_paused = true)]
async fn given_a_workspace_that_would_fail_and_the_mode_never_when_the_agent_runs_then_the_workspace_is_not_touched_and_the_run_ends()
 {
    let rig = Rig::new().with_files(FileSystemMode::Never, Vec::new());
    let model = Scripted::new(vec![Reply::text("fine")]);
    let flaky = FlakyWorkspace::new(rig.workspace.clone(), true, true);
    let mut parts = Parts::of(&rig);
    parts.workspace = flaky.clone();
    let node = react_over(&rig, parts, &[&model]);

    let event = Driver::run(&node).await.1;

    assert!(matches!(event, Event::Result(_)), "{event:?}");
    assert_eq!(flaky.loads(), 0);
    assert_eq!(flaky.saves(), 0);
}
