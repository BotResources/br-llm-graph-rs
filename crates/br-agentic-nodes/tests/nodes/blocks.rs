use br_agentic_contract::ToolSet;
use br_agentic_nodes::{Slot, StepLimit, Timeouts, Worker, WorkerParts};
use br_graph_contract::CancelToken;
use br_llm_messages::ClosureReason;
use std::time::Duration;

use crate::common::*;

#[tokio::test(start_paused = true)]
async fn given_the_public_blocks_when_a_runner_composes_its_own_loop_then_step_tool_round_drain_and_closing_work_together()
 {
    let (search, probe) = echo("search");
    let rig = Rig::new().with_inbox();
    let agent = rig.agent("agent").await;
    rig.start_as(agent, "go").await;
    let access = rig.hub.thread_access();
    let inbox = rig.hub.inbox();
    let cancel = CancelToken::new();
    let mut worker = Worker::new(WorkerParts {
        access: &access,
        inbox: Some(&inbox),
        context: rig.context_of(agent),
        author: author("custom"),
        system: br_llm_messages::PlainText::new("custom system").unwrap(),
        tools: ToolSet::new(vec![search], Vec::new()).unwrap(),
        step_limit: StepLimit::new(5).unwrap(),
        timeouts: Timeouts::new(Duration::from_secs(5), Duration::from_secs(5)).unwrap(),
        cancel: cancel.view(),
    });
    let model = Scripted::new(vec![Reply::call("search"), Reply::text("finished")]);
    let mut slot = Slot::new(list(&[&model]));
    let mut turn = None;

    let first = worker.step(&mut slot, &mut turn, None).await.unwrap();
    worker.tool_round(turn.unwrap(), &first).await.unwrap();
    rig.deliver_human(agent, "between steps");
    let drained = worker.drain().await.unwrap();
    let second = worker.step(&mut slot, &mut turn, None).await.unwrap();
    worker
        .close_turn(turn.unwrap(), ClosureReason::completed())
        .await
        .unwrap();

    assert_eq!(drained, 1);
    assert_eq!(text_of(&second), "finished");
    assert_eq!(worker.steps_remaining(), 3);
    assert!(slot.is_settled());
    assert_eq!(*probe.starts.lock().unwrap(), 1);
    let conversation = conversation(&rig.hub, &rig.thread);
    assert_eq!(
        kinds(ai_turns(&conversation)[0]),
        vec![
            "ModelResponse",
            "RuntimeToolResults",
            "HumanMessage",
            "ModelResponse",
            "TurnClosure"
        ]
    );
    assert_eq!(ai_turns(&conversation)[0].author().as_str(), "custom");
}
