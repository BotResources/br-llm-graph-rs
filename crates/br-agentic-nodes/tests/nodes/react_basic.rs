use br_agentic_contract::{AGENTIC, AgenticNode, Restart};
use br_graph_contract::{Event, Node};
use br_llm_messages::ClosureReason;

use crate::common::*;

#[test]
fn given_a_react_agent_when_asked_then_it_is_an_agentic_node_that_resets_its_counters() {
    let rig = Rig::new();
    let model = Scripted::new(vec![]);
    let without = rig.react(&[&model]);
    let with = Rig::new().with_inbox();
    let with = with.react(&[&model]);

    assert_eq!(
        (without.kind(), without.subkind()),
        (AGENTIC, "react_agent")
    );
    assert_eq!(without.name(), "agent");
    assert!(!without.has_inbox());
    assert!(with.has_inbox());
    assert_eq!(without.restart(), Restart::Resets);
}

#[tokio::test(start_paused = true)]
async fn given_a_final_answer_when_the_agent_runs_then_the_turn_is_completed_and_the_outcome_is_given()
 {
    let rig = Rig::new();
    rig.start_with("hello").await;
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    assert!(!update.step_limit);
    assert_eq!(update.verdict, None);
    let conversation = conversation(&rig.hub, &rig.thread);
    let turns = ai_turns(&conversation);
    assert_eq!(turns.len(), 1);
    assert_eq!(responses(turns[0]), vec!["done"]);
    assert!(matches!(
        closure(turns[0]),
        Some(ClosureReason::Completed { .. })
    ));
    assert_eq!(rig.outcome.outcomes().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_model_call_when_it_runs_then_the_response_is_open_on_the_thread_with_the_system_prompt_and_a_new_identity()
 {
    let rig = Rig::new();
    rig.start_with("hello").await;
    let model = Scripted::new(vec![Reply::text("one")]);
    let hub = rig.hub.clone();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = seen.clone();
    model.on_call(move |request| {
        log.lock().unwrap().push(hub.is_open(request.reply_entry()));
    });
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert_eq!(*seen.lock().unwrap(), vec![true]);
    let request = &model.requests()[0];
    assert_eq!(
        request.system().map(|system| system.as_str()),
        Some("system prompt of the agent")
    );
    assert_eq!(rig.prompts.asked("agent"), 1);
    assert!(!rig.hub.is_open(request.reply_entry()));
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_call_when_the_agent_runs_then_the_result_is_stored_and_the_next_step_sees_it()
{
    let (search, probe) = echo("search");
    let rig = Rig::new().with_tools(vec![search]);
    rig.start_with("hello").await;
    let model = Scripted::new(vec![
        Reply::call_with("search", "{\"q\":\"rust\"}"),
        Reply::text("found"),
    ]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "found");
    assert_eq!(*probe.arguments.lock().unwrap(), vec!["{\"q\":\"rust\"}"]);
    assert_eq!(*probe.cancel_views.lock().unwrap(), vec![false]);
    let conversation = conversation(&rig.hub, &rig.thread);
    let turn = ai_turns(&conversation)[0];
    assert_eq!(
        kinds(turn),
        vec![
            "ModelResponse",
            "RuntimeToolResults",
            "ModelResponse",
            "TurnClosure"
        ]
    );
    assert_eq!(results(turn).len(), 1);
    assert_eq!(model.requests()[1].tools().len(), 1);
    assert!(!rig.hub.is_open(model.requests()[1].reply_entry()));
}

#[tokio::test(start_paused = true)]
async fn given_two_agent_runs_when_the_second_starts_then_its_step_counter_starts_at_zero() {
    let rig = Rig::new().with_step_limit(1);
    rig.start_with("hello").await;
    let model = Scripted::new(vec![Reply::text("first"), Reply::text("second")]);
    let node = rig.react(&[&model]);

    let first = Driver::result(&node).await;
    let second = Driver::result(&node).await;

    assert_eq!(
        (first.text.as_str(), second.text.as_str()),
        ("first", "second")
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_node_without_an_author_when_it_runs_then_it_fails() {
    let rig = Rig::without_authors();
    rig.start_with("hello").await;
    let model = Scripted::new(vec![Reply::text("never")]);
    let node = rig.react(&[&model]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("no author name"), "{cause}");
    assert_eq!(model.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_an_author_in_the_build_when_the_agent_runs_then_the_turn_carries_it() {
    let rig = Rig::without_authors();
    rig.start_with("hello").await;
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]).with_author(author("Ada"));

    Driver::result(&node).await;

    let conversation = conversation(&rig.hub, &rig.thread);
    assert_eq!(ai_turns(&conversation)[0].author().as_str(), "Ada");
}

#[tokio::test(start_paused = true)]
async fn given_a_failing_outcome_port_when_the_agent_ends_then_it_fails_and_saves_no_file() {
    use br_agentic_contract::{FileSystemMode, WorkspaceFile};
    let mut rig = Rig::new().with_files(
        FileSystemMode::Always,
        vec![WorkspaceFile {
            path: "a.txt".to_owned(),
            content: b"a".to_vec(),
        }],
    );
    rig.outcome = std::sync::Arc::new(Recording::failing());
    rig.start_with("hello").await;
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    let event = Driver::run(&node).await.1;

    assert!(matches!(event, Event::Failed(_)));
    assert_eq!(rig.workspace.saves(), 0);
}
