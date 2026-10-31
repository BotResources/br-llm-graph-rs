use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use br_agentic_contract::memory::MemoryInbox;
use br_agentic_contract::{
    PortFuture, ResultStream, Tool, ToolBlock, ToolCode, ToolContent, ToolFault,
};
use br_graph_contract::CancelView;
use br_llm_clients_core::ClientError;
use br_llm_messages::{AgentId, OpaqueJson};

use crate::access_spy::{AccessSpy, Op};
use crate::common::*;
use crate::kit::{Parts, react_over};

struct Delivers {
    inbox: Arc<MemoryInbox>,
    agent: AgentId,
    done: AtomicBool,
}

impl ToolCode for Delivers {
    fn call<'a>(
        &'a self,
        _arguments: &'a OpaqueJson,
        _cancel: CancelView,
        _result: &'a mut dyn ResultStream,
    ) -> PortFuture<'a, Result<ToolContent, ToolFault>> {
        Box::pin(async move {
            if !self.done.swap(true, Ordering::SeqCst) {
                self.inbox
                    .deliver_entry(self.agent, inbox_entry("a late request"));
            }
            Ok(ToolContent::new(
                vec![ToolBlock::text("delivered").unwrap()],
                false,
            ))
        })
    }
}

fn spied(rig: &Rig, op: Op) -> (Parts, Arc<AccessSpy>) {
    let spy = Arc::new(AccessSpy::over(rig.access.clone()).failing(op));
    (Parts::of(rig).with_access(spy.clone()), spy)
}

#[tokio::test(start_paused = true)]
async fn given_an_add_of_an_input_turn_that_fails_when_the_drain_stores_a_request_then_the_instance_fails()
 {
    let rig = Rig::new().with_inbox();
    rig.inbox.deliver_entry(rig.agent, inbox_entry("one more"));
    let (parts, spy) = spied(&rig, Op::AddInputTurn);
    let model = Scripted::new(vec![Reply::text("never")]);
    let node = react_over(&rig, parts, &[&model]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("AddInputTurn failed"), "{cause}");
    assert_eq!(spy.count(Op::AddInputTurn), 1);
    assert_eq!(model.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_an_add_of_an_entry_that_fails_when_the_drain_stores_a_request_in_the_open_turn_then_the_instance_fails()
 {
    let (quick, _) = echo("a");
    let rig = Rig::new().with_inbox();
    let deliver = Tool::new(
        spec("deliver"),
        Arc::new(Delivers {
            inbox: rig.inbox.clone(),
            agent: rig.agent,
            done: AtomicBool::new(false),
        }),
    );
    let rig = rig.with_tools(vec![deliver, quick]);
    let (parts, spy) = spied(&rig, Op::AddEntry);
    let model = Scripted::new(vec![
        Reply::call("deliver"),
        Reply::call("a"),
        Reply::call("a"),
        Reply::text("never"),
    ]);
    let node = react_over(&rig, parts, &[&model]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("AddEntry failed"), "{cause}");
    assert_eq!(spy.count(Op::AddEntry), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_closure_that_fails_when_the_turn_of_a_final_response_closes_then_the_instance_fails()
 {
    let rig = Rig::new();
    let (parts, spy) = spied(&rig, Op::CloseTurn);
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = react_over(&rig, parts, &[&model]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("CloseTurn failed"), "{cause}");
    assert!(spy.count(Op::CloseTurn) >= 1);
    assert_eq!(rig.outcome.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_an_open_of_a_response_that_fails_when_a_step_starts_then_the_instance_fails_before_the_model_is_called()
 {
    let rig = Rig::new();
    let (parts, spy) = spied(&rig, Op::OpenResponse);
    let model = Scripted::new(vec![Reply::text("never")]);
    let node = react_over(&rig, parts, &[&model]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("OpenResponse failed"), "{cause}");
    assert_eq!(model.calls(), 0);
    assert_eq!(spy.count(Op::OpenResponse), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_completion_of_a_response_that_fails_when_the_model_answers_then_the_instance_fails_without_a_retry()
 {
    let rig = Rig::new();
    let (parts, spy) = spied(&rig, Op::CompleteResponse);
    let model = Scripted::new(vec![Reply::text("done"), Reply::text("again")]);
    let node = react_over(&rig, parts, &[&model]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("CompleteResponse failed"), "{cause}");
    assert_eq!(spy.count(Op::CompleteResponse), 1);
    assert_eq!(model.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_discard_of_a_response_that_fails_when_the_first_client_fails_then_the_instance_fails_and_the_next_client_is_not_called()
 {
    let rig = Rig::new();
    let (parts, spy) = spied(&rig, Op::DiscardResponse);
    let first = Scripted::new(vec![Reply::Fail(ClientError::Transport)]);
    let second = Scripted::new(vec![Reply::text("never")]);
    let node = react_over(&rig, parts, &[&first, &second]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("DiscardResponse failed"), "{cause}");
    assert_eq!(second.calls(), 0);
    assert!(spy.count(Op::DiscardResponse) >= 1);
}

#[tokio::test(start_paused = true)]
async fn given_an_open_of_a_result_that_fails_when_a_tool_round_starts_then_the_instance_fails_before_the_tool_runs()
 {
    let (tool_a, probe) = echo("a");
    let rig = Rig::new().with_tools(vec![tool_a]);
    let (parts, spy) = spied(&rig, Op::OpenResult);
    let model = Scripted::new(vec![Reply::call("a"), Reply::text("never")]);
    let node = react_over(&rig, parts, &[&model]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("OpenResult failed"), "{cause}");
    assert_eq!(*probe.starts.lock().unwrap(), 0);
    assert_eq!(spy.count(Op::OpenResult), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_completion_of_a_result_that_fails_when_a_tool_ends_then_the_instance_fails_without_a_retry()
 {
    let (tool_a, probe) = echo("a");
    let rig = Rig::new().with_tools(vec![tool_a]);
    let (parts, spy) = spied(&rig, Op::CompleteResult);
    let model = Scripted::new(vec![Reply::call("a"), Reply::text("never")]);
    let node = react_over(&rig, parts, &[&model]);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("CompleteResult failed"), "{cause}");
    assert_eq!(*probe.starts.lock().unwrap(), 1);
    assert_eq!(spy.count(Op::CompleteResult), 1);
    assert_eq!(model.calls(), 1);
}
