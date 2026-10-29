use std::sync::Arc;
use std::time::Duration;

use br_agentic_nodes::{FallbackList, ModelClient, ReactAgent};
use br_llm_clients_core::ClientError;

use crate::common::*;

fn unavailable() -> Reply {
    Reply::Fail(ClientError::Unavailable { retry_after: None })
}

fn thoughtful(clients: &[&Arc<Scripted>]) -> FallbackList {
    FallbackList::new(
        clients
            .iter()
            .map(|scripted| {
                ModelClient::new(
                    (*scripted).clone(),
                    br_agentic_nodes::ThinkingExpectation::Expected,
                )
            })
            .collect(),
    )
    .unwrap()
}

fn agent_with(rig: &Rig, list: FallbackList) -> ReactAgent<Doc> {
    ReactAgent::new("agent", rig.ports(), list, rig.settings.clone())
}

#[tokio::test(start_paused = true)]
async fn given_one_client_when_the_first_call_fails_then_there_is_no_retry_and_the_instance_fails()
{
    let rig = Rig::new();
    rig.start_with("go").await;
    let only = Scripted::new(vec![unavailable(), Reply::text("never")]);
    let node = rig.react(&[&only]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("provider is unavailable"), "{cause}");
    assert_eq!(only.calls(), 1);
    assert!(!rig.hub.is_open(only.requests()[0].reply_entry()));
    let conversation = conversation(&rig.hub, &rig.thread);
    assert!(ai_turns(&conversation).is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_two_clients_when_the_first_call_fails_then_the_next_client_serves_it_on_a_new_response()
 {
    let rig = Rig::new();
    rig.start_with("go").await;
    let first = Scripted::new(vec![unavailable()]);
    let second = Scripted::new(vec![Reply::text("from the second")]);
    let node = rig.react(&[&first, &second]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "from the second");
    let failed = first.requests()[0].reply_entry();
    let served = second.requests()[0].reply_entry();
    assert_ne!(failed, served);
    assert!(!rig.hub.is_open(failed));
    let conversation = conversation(&rig.hub, &rig.thread);
    assert_eq!(
        responses(ai_turns(&conversation)[0]),
        vec!["from the second"]
    );
}

#[tokio::test(start_paused = true)]
async fn given_three_clients_when_two_calls_fail_then_the_third_serves_and_the_list_is_used_in_order()
 {
    let rig = Rig::new();
    rig.start_with("go").await;
    let first = Scripted::new(vec![unavailable()]);
    let second = Scripted::new(vec![unavailable()]);
    let third = Scripted::new(vec![Reply::text("third")]);
    let node = rig.react(&[&first, &second, &third]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "third");
    assert_eq!((first.calls(), second.calls(), third.calls()), (1, 1, 1));
}

#[tokio::test(start_paused = true)]
async fn given_a_list_used_up_when_every_call_fails_then_the_last_error_is_a_fault() {
    let rig = Rig::new();
    rig.start_with("go").await;
    let first = Scripted::new(vec![unavailable()]);
    let second = Scripted::new(vec![Reply::Fail(ClientError::RateLimited {
        retry_after: None,
    })]);
    let node = rig.react(&[&first, &second]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("rate limited"), "{cause}");
}

#[tokio::test(start_paused = true)]
async fn given_a_client_that_worked_when_a_later_call_fails_then_the_instance_fails_and_the_next_client_is_not_used()
 {
    let (search, _) = echo("search");
    let rig = Rig::new().with_tools(vec![search]);
    rig.start_with("go").await;
    let first = Scripted::new(vec![Reply::call("search"), unavailable()]);
    let second = Scripted::new(vec![Reply::text("never")]);
    let node = rig.react(&[&first, &second]);

    let (_, event) = Driver::run(&node).await;

    assert!(matches!(event, br_graph_contract::Event::Failed(_)));
    assert_eq!(second.calls(), 0);
    let conversation = conversation(&rig.hub, &rig.thread);
    assert!(matches!(
        closure(ai_turns(&conversation)[0]),
        Some(br_llm_messages::ClosureReason::Failed { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn given_a_client_that_worked_when_the_next_step_runs_then_it_keeps_the_same_client() {
    let (search, _) = echo("search");
    let rig = Rig::new().with_tools(vec![search]);
    rig.start_with("go").await;
    let first = Scripted::new(vec![unavailable()]);
    let second = Scripted::new(vec![Reply::call("search"), Reply::text("done")]);
    let node = rig.react(&[&first, &second]);

    Driver::result(&node).await;

    assert_eq!((first.calls(), second.calls()), (1, 2));
}

#[tokio::test(start_paused = true)]
async fn given_a_first_call_over_its_timeout_when_the_list_has_another_client_then_it_switches() {
    let rig = Rig::new().with_timeouts(Duration::from_secs(3), Duration::from_secs(3));
    rig.start_with("go").await;
    let slow = Scripted::new(vec![Reply::Hang]);
    let quick = Scripted::new(vec![Reply::text("quick")]);
    let node = rig.react(&[&slow, &quick]);
    let started = tokio::time::Instant::now();

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "quick");
    assert_eq!(started.elapsed(), Duration::from_secs(3));
    assert!(!rig.hub.is_open(slow.requests()[0].reply_entry()));
}

#[tokio::test(start_paused = true)]
async fn given_a_first_call_over_its_timeout_when_the_list_has_one_client_then_the_instance_fails()
{
    let rig = Rig::new().with_timeouts(Duration::from_secs(3), Duration::from_secs(3));
    rig.start_with("go").await;
    let slow = Scripted::new(vec![Reply::Hang]);
    let node = rig.react(&[&slow]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("timeout"), "{cause}");
}

#[tokio::test(start_paused = true)]
async fn given_a_later_call_over_its_timeout_when_the_next_client_exists_then_the_instance_fails() {
    let (search, _) = echo("search");
    let rig = Rig::new()
        .with_tools(vec![search])
        .with_timeouts(Duration::from_secs(3), Duration::from_secs(3));
    rig.start_with("go").await;
    let first = Scripted::new(vec![Reply::call("search"), Reply::Hang]);
    let second = Scripted::new(vec![Reply::text("never")]);
    let node = rig.react(&[&first, &second]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("timeout"), "{cause}");
    assert_eq!(second.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_client_that_should_think_when_the_response_holds_no_thinking_then_the_next_client_serves()
 {
    let rig = Rig::new();
    rig.start_with("go").await;
    let silent = Scripted::new(vec![Reply::text("no thought")]);
    let thinker = Scripted::new(vec![Reply::thinking(Reply::text("thought"))]);
    let node = agent_with(&rig, thoughtful(&[&silent, &thinker]));

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "thought");
    assert!(!rig.hub.is_open(silent.requests()[0].reply_entry()));
    let conversation = conversation(&rig.hub, &rig.thread);
    assert_eq!(responses(ai_turns(&conversation)[0]), vec!["thought"]);
}

#[tokio::test(start_paused = true)]
async fn given_one_client_that_should_think_when_the_response_holds_no_thinking_then_the_instance_fails()
 {
    let rig = Rig::new();
    rig.start_with("go").await;
    let silent = Scripted::new(vec![Reply::text("no thought")]);
    let node = agent_with(&rig, thoughtful(&[&silent]));

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("should have thought"), "{cause}");
}

#[tokio::test(start_paused = true)]
async fn given_a_later_call_that_should_think_when_the_response_holds_no_thinking_then_the_instance_fails()
 {
    let (search, _) = echo("search");
    let rig = Rig::new().with_tools(vec![search]);
    rig.start_with("go").await;
    let thinker = Scripted::new(vec![
        Reply::thinking(Reply::call("search")),
        Reply::text("silent"),
    ]);
    let spare = Scripted::new(vec![Reply::thinking(Reply::text("never"))]);
    let node = agent_with(&rig, thoughtful(&[&thinker, &spare]));

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("should have thought"), "{cause}");
    assert_eq!(spare.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_client_that_does_not_expect_thinking_when_the_response_holds_none_then_the_call_works()
 {
    let rig = Rig::new();
    rig.start_with("go").await;
    let plain = Scripted::new(vec![Reply::text("plain")]);
    let node = rig.react(&[&plain]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "plain");
}
