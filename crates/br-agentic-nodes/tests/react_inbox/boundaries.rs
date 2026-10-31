use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use br_agentic_nodes::{AuthorSource, ModelClient, ReactAgent, ThinkingExpectation};
use br_llm_clients_core::{
    BoxFuture, CallOutcome, CallReport, Client, ClientRequest, ClientSettings, StreamSink,
};
use br_llm_messages::{
    ContentBlock, ExecutionStatus, ModelResponse, OpaqueJson, ProviderToolExecutionRecord,
    ResponseEnding, ResponseMetadata, RuntimeToolCall, ToolName,
};

use crate::common::*;
use crate::kit::*;
use crate::spy::{Items, Spy};

fn rig_with_noop() -> Rig {
    let (noop, _) = echo("noop");
    Rig::new().with_tools(vec![noop])
}

struct PendingExecution {
    rest: Arc<Scripted>,
    first: AtomicBool,
}

impl Client for PendingExecution {
    fn call<'a>(
        &'a self,
        request: &'a ClientRequest,
        sink: &'a mut dyn StreamSink,
    ) -> BoxFuture<'a, CallOutcome> {
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if self.first.swap(false, Ordering::SeqCst) {
                let blocks = vec![
                    ContentBlock::RuntimeToolCall(
                        RuntimeToolCall::new(
                            block_id(),
                            ToolName::new("noop").unwrap(),
                            OpaqueJson::parse("{}").unwrap(),
                        )
                        .unwrap(),
                    ),
                    ContentBlock::ProviderToolExecutionRecord(
                        ProviderToolExecutionRecord::new(block_id())
                            .with_status(ExecutionStatus::Pending),
                    ),
                ];
                let response = ModelResponse::new(
                    request.reply_entry(),
                    blocks,
                    ResponseEnding::tool_use(),
                    ResponseMetadata::new(),
                )
                .unwrap();
                return CallOutcome::success(CallReport::new(Duration::ZERO), response).unwrap();
            }
            self.rest.call(request, sink).await
        })
    }

    fn settings(&self) -> &ClientSettings {
        self.rest.settings()
    }
}

fn pending_execution_node(rig: &Rig, spy: &Arc<Spy>, rest: &Arc<Scripted>) -> ReactAgent<Doc> {
    let client = ModelClient::new(
        Arc::new(PendingExecution {
            rest: rest.clone(),
            first: AtomicBool::new(true),
        }),
        ThinkingExpectation::NotExpected,
    );
    let mut spec = rig.react_spec(br_agentic_nodes::FallbackList::new(vec![client]).unwrap());
    spec.author = AuthorSource::Fixed(author("Agent"));
    ReactAgent::new(
        "agent",
        rig.ports().with_inbox(spy.clone()),
        rig.settings.clone(),
        spec,
    )
}

#[tokio::test(start_paused = true)]
async fn given_an_item_read_beside_the_loop_when_a_tool_run_ends_then_it_goes_in_the_open_ai_turn_after_the_results()
 {
    let rig = rig_with_noop();
    let spy = Spy::new(&rig);
    let (x, x_id) = human("marker-after-a-tool-run");
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Give(batch(vec![item(&x)], None)),
    ]);
    let model = Scripted::new(vec![
        Reply::call("noop"),
        Reply::call("noop"),
        Reply::text("done"),
    ]);
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;

    let conversation = rig.conversation();
    assert_eq!(ai_turns(&conversation).len(), 1);
    let placed = held(&conversation)
        .into_iter()
        .find(|entry| entry.id == x_id)
        .unwrap();
    assert!(placed.in_ai_turn);
    let results_id = results(ai_turns(&conversation)[0])[0].id();
    assert_eq!(
        position_of(&conversation, x_id),
        position_of(&conversation, results_id).map(|at| at + 1)
    );
    assert!(!history_text(&model.requests()[0]).contains("marker-after-a-tool-run"));
    assert!(history_text(&model.requests()[1]).contains("marker-after-a-tool-run"));
    assert!(rig.inbox.is_read(rig.agent, x_id) || spy.times_marked(x_id) == 1);
    assert_eq!(spy.times_marked(x_id), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_response_with_a_provider_continuation_when_its_tool_run_ends_then_the_queue_is_not_drained()
 {
    let rig = rig_with_noop();
    let spy = Spy::new(&rig);
    let (x, x_id) = human("marker-after-a-continuation");
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Give(batch(vec![item(&x)], None)),
    ]);
    let model = Scripted::new(vec![
        Reply::call_then_continuation("noop"),
        Reply::call("noop"),
        Reply::text("done"),
    ]);
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;

    let requests = model.requests();
    assert!(!history_text(&requests[1]).contains("marker-after-a-continuation"));
    assert!(history_text(&requests[2]).contains("marker-after-a-continuation"));
    assert_eq!(count_of(&rig.conversation(), x_id), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_response_that_leaves_a_provider_execution_pending_when_its_tool_run_ends_then_the_queue_is_not_drained()
 {
    let rig = rig_with_noop();
    let spy = Spy::new(&rig);
    let (x, x_id) = human("marker-after-a-pending-execution");
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Give(batch(vec![item(&x)], None)),
    ]);
    let rest = Scripted::new(vec![Reply::call("noop"), Reply::text("done")]);
    let node = pending_execution_node(&rig, &spy, &rest);

    Driver::result(&node).await;

    let requests = rest.requests();
    assert!(!history_text(&requests[0]).contains("marker-after-a-pending-execution"));
    assert!(history_text(&requests[1]).contains("marker-after-a-pending-execution"));
    assert_eq!(count_of(&rig.conversation(), x_id), 1);
}

#[tokio::test(start_paused = true)]
async fn given_no_step_left_after_a_final_response_when_the_agent_ends_then_the_queue_is_not_drained()
 {
    let rig = rig_with_noop().with_step_limit(1);
    let spy = Spy::new(&rig);
    let (x, x_id) = human("marker-with-no-step-left");
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Give(batch(vec![item(&x)], None)),
    ]);
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.steps, Some(1));
    assert!(!update.step_limit);
    assert_eq!(count_of(&rig.conversation(), x_id), 0);
    assert_eq!(spy.times_marked(x_id), 0);
}

#[tokio::test(start_paused = true)]
async fn given_the_step_limit_stops_a_response_with_tool_calls_when_the_agent_ends_then_the_queue_is_not_drained()
 {
    let rig = rig_with_noop().with_step_limit(1);
    let spy = Spy::new(&rig);
    let (x, x_id) = human("marker-at-the-step-limit");
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Give(batch(vec![item(&x)], None)),
    ]);
    let model = Scripted::new(vec![Reply::call("noop")]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert!(update.step_limit);
    assert_eq!(count_of(&rig.conversation(), x_id), 0);
    assert_eq!(spy.times_marked(x_id), 0);
    assert!(results(ai_turns(&rig.conversation())[0]).is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_step_left_after_a_final_response_when_the_agent_ends_then_the_same_item_is_drained()
 {
    let rig = rig_with_noop().with_step_limit(2);
    let spy = Spy::new(&rig);
    let (x, x_id) = human("marker-with-a-step-left");
    spy.plan_items(vec![
        Items::Give(empty_batch()),
        Items::Give(batch(vec![item(&x)], None)),
    ]);
    let model = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.steps, Some(2));
    assert_eq!(count_of(&rig.conversation(), x_id), 1);
    assert_eq!(spy.times_marked(x_id), 1);
}
