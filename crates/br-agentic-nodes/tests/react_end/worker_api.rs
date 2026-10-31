use std::time::Duration;

use br_agentic_contract::{AiTurnRef, InboxContext, ToolSet};
use br_agentic_nodes::{Drained, Slot, StepLimit, Timeouts, Worker, WorkerParts};
use br_graph_contract::CancelToken;
use br_llm_messages::{ClosureReason, PlainText, TurnId};

use crate::access_spy::{AccessSpy, Op};
use crate::common::*;

fn parts<'a>(
    rig: &'a Rig,
    access: &'a dyn br_agentic_contract::ThreadAccess,
    cancel: &CancelToken,
) -> WorkerParts<'a> {
    WorkerParts {
        access,
        inbox: None,
        context: InboxContext {
            instance: instance_id(),
            thread: rig.thread,
            agent: rig.agent,
        },
        step_limit: StepLimit::new(5).unwrap(),
        timeouts: Timeouts::new(Duration::from_secs(5), Duration::from_secs(5)).unwrap(),
        cancel: cancel.view(),
    }
}

#[tokio::test(start_paused = true)]
async fn given_the_public_blocks_when_a_runner_author_drives_one_pass_then_the_blocks_do_the_loop()
{
    let (tool_a, probe) = echo("a");
    let rig = Rig::new();
    let cancel = CancelToken::new();
    let model = Scripted::new(vec![Reply::call("a"), Reply::text("final")]);
    let mut slot = Slot::new(list(&[&model]));
    let mut worker = Worker::open(parts(&rig, &*rig.access, &cancel))
        .await
        .unwrap()
        .with_author(author("Builder"))
        .with_system(PlainText::new("the system prompt").unwrap())
        .with_tools(ToolSet::new(vec![tool_a], Vec::new()).unwrap());
    assert!(!worker.has_inbox());
    assert_eq!(worker.steps_remaining(), 5);
    assert_eq!(worker.conversation().turns().len(), 1);
    let mut turn: Option<TurnId> = None;

    let first = worker.step(&mut slot, &mut turn, None).await.unwrap();
    let open = turn.expect("a step opens the turn");
    worker.tool_round(open, &first).await.unwrap();
    let drained = worker.drain().await.unwrap();
    let second = worker.step(&mut slot, &mut turn, None).await.unwrap();
    let target = AiTurnRef {
        id: open,
        agent: rig.agent,
        author: author("Builder"),
    };
    worker
        .close_turn(&target, ClosureReason::completed())
        .await
        .unwrap();

    assert_eq!(drained, Drained::default());
    assert_eq!(worker.steps_made(), 2);
    assert_eq!(worker.steps_remaining(), 3);
    assert_eq!(*probe.starts.lock().unwrap(), 1);
    assert_eq!(text_of(&second), "final");
    let requests = model.requests();
    assert_eq!(requests[0].system().unwrap().as_str(), "the system prompt");
    assert_eq!(requests[0].tools().len(), 1);
    assert!(history_text(&requests[1]).contains("a done"));
    let kept = rig.conversation();
    assert_eq!(&kept, worker.conversation());
    let turns = ai_turns(&kept);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].author().as_str(), "Builder");
    assert_eq!(responses(turns[0]).len(), 2);
    assert_eq!(texts_of_results(turns[0]).len(), 1);
    assert!(matches!(
        closure(turns[0]),
        Some(ClosureReason::Completed { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn given_a_closure_for_a_turn_the_copy_does_not_hold_when_the_worker_closes_it_then_it_is_refused_and_the_thread_access_gets_nothing()
 {
    let rig = Rig::new();
    let cancel = CancelToken::new();
    let spy = AccessSpy::over(rig.access.clone());
    let mut worker = Worker::open(parts(&rig, &spy, &cancel))
        .await
        .unwrap()
        .with_author(author("Builder"));
    let unknown = AiTurnRef {
        id: turn_id(),
        agent: rig.agent,
        author: author("Builder"),
    };

    let refused = worker
        .close_turn(&unknown, ClosureReason::completed())
        .await;

    assert!(refused.is_err());
    assert_eq!(spy.count(Op::CloseTurn), 0);
    assert!(ai_turns(&rig.conversation()).is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_turn_already_closed_when_the_worker_closes_it_again_then_the_copy_refuses_and_the_thread_access_gets_no_second_closure()
 {
    let rig = Rig::new();
    let cancel = CancelToken::new();
    let spy = AccessSpy::over(rig.access.clone());
    let model = Scripted::new(vec![Reply::text("final")]);
    let mut slot = Slot::new(list(&[&model]));
    let mut worker = Worker::open(parts(&rig, &spy, &cancel))
        .await
        .unwrap()
        .with_author(author("Builder"));
    let mut turn: Option<TurnId> = None;
    worker.step(&mut slot, &mut turn, None).await.unwrap();
    let target = AiTurnRef {
        id: turn.unwrap(),
        agent: rig.agent,
        author: author("Builder"),
    };
    worker
        .close_turn(&target, ClosureReason::completed())
        .await
        .unwrap();

    let again = worker.close_turn(&target, ClosureReason::completed()).await;

    assert!(again.is_err());
    assert_eq!(spy.count(Op::CloseTurn), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_thread_access_that_fails_the_read_when_the_worker_is_opened_then_it_is_a_fault() {
    let rig = Rig::new();
    let cancel = CancelToken::new();
    let spy = AccessSpy::over(rig.access.clone()).failing(Op::Read);

    let opened = Worker::open(parts(&rig, &spy, &cancel)).await;

    assert!(opened.is_err());
    assert_eq!(spy.count(Op::Read), 1);
}
