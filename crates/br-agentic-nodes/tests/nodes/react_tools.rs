use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_agentic_contract::memory::MemoryHub;
use br_agentic_contract::{ResultStream, ThreadRef, Tool, ToolCode, ToolContent, ToolFault};
use br_graph_contract::{CancelView, Event};
use br_llm_clients_core::BoxFuture;
use br_llm_messages::{ClosureReason, OpaqueJson};

use crate::common::*;

struct Observer {
    hub: MemoryHub,
    thread: ThreadRef,
    delay: Duration,
    stored_when_done: Mutex<Option<usize>>,
}

impl ToolCode for Observer {
    fn call<'a>(
        &'a self,
        _arguments: &'a OpaqueJson,
        _cancel: CancelView,
        _result: &'a mut dyn ResultStream,
    ) -> BoxFuture<'a, Result<ToolContent, ToolFault>> {
        Box::pin(async move {
            tokio::time::sleep(self.delay).await;
            let conversation = conversation(&self.hub, &self.thread);
            let stored: usize = ai_turns(&conversation)
                .into_iter()
                .map(|turn| results(turn).len())
                .sum();
            *self.stored_when_done.lock().unwrap() = Some(stored);
            Ok(ToolContent::new(vec![text_block("observed")], false))
        })
    }
}

#[tokio::test(start_paused = true)]
async fn given_two_calls_in_one_response_when_the_round_runs_then_both_start_together() {
    let (first, first_probe) = tool(
        "first",
        Behavior::Slow(Duration::from_secs(10), "one".into()),
    );
    let (second, second_probe) = tool(
        "second",
        Behavior::Slow(Duration::from_secs(10), "two".into()),
    );
    let rig = Rig::new()
        .with_tools(vec![first, second])
        .with_timeouts(Duration::from_secs(5), Duration::from_secs(60));
    rig.start_with("go").await;
    let model = Scripted::new(vec![
        Reply::calls(&["first", "second"]),
        Reply::text("done"),
    ]);
    let node = rig.react(&[&model]);
    let started = tokio::time::Instant::now();

    Driver::result(&node).await;

    assert_eq!(started.elapsed(), Duration::from_secs(10));
    assert_eq!(*first_probe.starts.lock().unwrap(), 1);
    assert_eq!(*second_probe.starts.lock().unwrap(), 1);
    let conversation = conversation(&rig.hub, &rig.thread);
    assert_eq!(results(ai_turns(&conversation)[0]).len(), 2);
}

#[tokio::test(start_paused = true)]
async fn given_a_fast_call_when_a_slow_call_still_runs_then_the_fast_result_is_already_stored() {
    let (fast, _) = echo("fast");
    let rig = Rig::new();
    let observer = Arc::new(Observer {
        hub: rig.hub.clone(),
        thread: rig.thread,
        delay: Duration::from_secs(10),
        stored_when_done: Mutex::new(None),
    });
    let slow = Tool::new(spec("slow"), observer.clone());
    let rig = rig
        .with_tools(vec![fast, slow])
        .with_timeouts(Duration::from_secs(5), Duration::from_secs(60));
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::calls(&["fast", "slow"]), Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert_eq!(*observer.stored_when_done.lock().unwrap(), Some(1));
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_the_node_does_not_hold_when_the_model_calls_it_then_the_result_is_an_error_that_names_it()
 {
    let rig = Rig::new();
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::call("ghost"), Reply::text("done")]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    let conversation = conversation(&rig.hub, &rig.thread);
    let stored = texts_of_results(ai_turns(&conversation)[0]);
    assert_eq!(stored.len(), 1);
    assert!(stored[0].0);
    assert!(stored[0].1.contains("ghost"), "{}", stored[0].1);
}

#[tokio::test(start_paused = true)]
async fn given_a_call_over_its_timeout_when_the_round_runs_then_the_result_tells_the_timeout_and_the_loop_goes_on()
 {
    let (slow, probe) = tool("slow", Behavior::Hang);
    let rig = Rig::new()
        .with_tools(vec![slow])
        .with_timeouts(Duration::from_secs(5), Duration::from_secs(2));
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::call("slow"), Reply::text("done")]);
    let node = rig.react(&[&model]);
    let started = tokio::time::Instant::now();

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    assert_eq!(started.elapsed(), Duration::from_secs(2));
    assert_eq!(*probe.starts.lock().unwrap(), 1);
    let conversation = conversation(&rig.hub, &rig.thread);
    let stored = texts_of_results(ai_turns(&conversation)[0]);
    assert!(stored[0].0);
    assert!(stored[0].1.contains("timeout"), "{}", stored[0].1);
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_error_result_when_the_round_ends_then_it_is_stored_with_its_flag_and_the_loop_goes_on()
 {
    let (broken, _) = tool("broken", Behavior::ErrorText("no luck".into()));
    let rig = Rig::new().with_tools(vec![broken]);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::call("broken"), Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let conversation = conversation(&rig.hub, &rig.thread);
    let stored = texts_of_results(ai_turns(&conversation)[0]);
    assert!(stored[0].0);
    assert!(stored[0].1.contains("no luck"));
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_fault_when_the_round_runs_then_the_instance_fails_and_stops_the_other_calls()
{
    let (broken, _) = tool("broken", Behavior::Fault);
    let (hanging, hang_probe) = tool("hanging", Behavior::Hang);
    let rig = Rig::new().with_tools(vec![broken, hanging]);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::calls(&["hanging", "broken"])]);
    let node = rig.react(&[&model]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("the tool broke"), "{cause}");
    assert_eq!(*hang_probe.starts.lock().unwrap(), 1);
    let conversation = conversation(&rig.hub, &rig.thread);
    let turn = ai_turns(&conversation)[0];
    match closure(turn) {
        Some(ClosureReason::Failed { .. }) => {}
        other => panic!("expected Failed, got {other:?}"),
    }
    assert!(
        closure(turn)
            .unwrap()
            .detail()
            .unwrap()
            .contains("the tool broke")
    );
    assert!(texts_of_results(turn).is_empty());
    assert_eq!(rig.outcome.outcomes().len(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_that_panics_when_the_round_runs_then_the_instance_fails_and_the_turn_is_failed()
 {
    let (broken, _) = tool("broken", Behavior::Panic);
    let rig = Rig::new().with_tools(vec![broken]);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::call("broken")]);
    let node = rig.react(&[&model]);

    let (_, event) = Driver::run(&node).await;

    assert!(matches!(event, Event::Failed(_)));
    let conversation = conversation(&rig.hub, &rig.thread);
    assert!(matches!(
        closure(ai_turns(&conversation)[0]),
        Some(ClosureReason::Failed { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_name_equal_to_a_mail_tool_when_the_node_has_an_inbox_then_the_instance_fails()
{
    let (clash, _) = echo("send_mail");
    let rig = Rig::new().with_inbox().with_tools(vec![clash]);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::text("never")]);
    let node = rig.react(&[&model]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("send_mail"), "{cause}");
    assert_eq!(model.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_node_without_an_inbox_when_it_runs_then_the_model_is_offered_only_the_tools_of_the_port()
 {
    let (search, _) = echo("search");
    let rig = Rig::new().with_tools(vec![search]);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let names: Vec<String> = model.requests()[0]
        .tools()
        .iter()
        .map(|tool| tool.name().as_str().to_owned())
        .collect();
    assert_eq!(names, vec!["search"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_node_with_an_inbox_when_it_runs_then_the_model_is_offered_the_mail_tools_too() {
    let (search, _) = echo("search");
    let rig = Rig::new().with_inbox().with_tools(vec![search]);
    rig.start_with("go").await;
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let mut names: Vec<String> = model.requests()[0]
        .tools()
        .iter()
        .map(|tool| tool.name().as_str().to_owned())
        .collect();
    names.sort();
    assert_eq!(names, vec!["get_mail_files", "search", "send_mail"]);
}
