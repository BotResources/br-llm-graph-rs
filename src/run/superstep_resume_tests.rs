//! A superstep is applied as a whole: when one of its nodes fails, nothing of
//! it reaches the state, and a resume skips the nodes that finished.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use futures_util::future::join;

use crate::graph::{Context, FnEdge, FnNode, Graph, GraphBuilder, Map, Node, NodeFuture};
use crate::observe::NoopObserver;
use crate::run::Checkpoint;
use crate::run::counted::{Counted, CountedBody};
use crate::run::gates::{GatedBody, Gates};
use crate::run::inbox::channel;
use crate::run::outcome::{Outcome, RunFailure};
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::{Config, State, Value};
use crate::update::Update;

fn ctx() -> Context {
    crate::testkit::context(Arc::new(NoopObserver))
}

/// Appends `mark` to `log` and counts its calls.
fn counted_log(mark: &'static str, calls: &Arc<AtomicUsize>) -> impl Node + use<> {
    let calls = calls.clone();
    FnNode::new(
        move |_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Ok(vec![Update::Append {
                    key: key("log"),
                    value: Value::str(mark),
                }])
            })
        },
    )
}

/// Fails while `failing` holds, then returns nothing.
fn failing_while(failing: &Arc<AtomicBool>) -> impl Node + use<> {
    let failing = failing.clone();
    FnNode::new(
        move |_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            let fail = failing.load(Ordering::SeqCst);
            Box::pin(async move {
                if fail {
                    Err("the node failed".into())
                } else {
                    Ok(Vec::new())
                }
            })
        },
    )
}

/// `start` sets `count` to 1, then `ok` and `bad` run in one superstep.
fn siblings(ok: impl Node + 'static, bad: impl Node + 'static) -> Graph {
    GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), set_count_node(1))
        .node(nid("ok"), ok)
        .node(nid("bad"), bad)
        .edge(nid("start"), edge(vec![to("ok"), to("bad")]))
        .edge(nid("ok"), edge(vec![end("done")]))
        .edge(nid("bad"), edge(vec![end("done")]))
        .build()
        .unwrap()
}

async fn first_run(graph: &Graph) -> RunFailure {
    first_run_from(graph, base_state()).await
}

async fn first_run_from(graph: &Graph, state: State) -> RunFailure {
    let (_sender, mut inbox) = channel();
    run(graph, &config(), state, None, &ctx(), &mut inbox)
        .await
        .err()
        .unwrap()
}

async fn resume(graph: &Graph, checkpoint: Checkpoint) -> State {
    let (_sender, mut inbox) = channel();
    let resumed = ctx().with_pending(checkpoint.pending);
    let outcome = run(
        graph,
        &config(),
        checkpoint.state,
        Some(checkpoint.cursor),
        &resumed,
        &mut inbox,
    )
    .await;
    match outcome {
        Ok(Outcome::Finished { state, .. }) => state,
        Ok(_) => panic!("expected finished"),
        Err(failure) => panic!("expected finished, got {}", failure.error),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_failing_sibling_when_resumed_then_the_other_sibling_is_applied_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let failing = Arc::new(AtomicBool::new(true));
    let graph = siblings(counted_log("ok", &calls), failing_while(&failing));
    let failure = first_run(&graph).await;
    failing.store(false, Ordering::SeqCst);
    let state = resume(&graph, failure.checkpoint).await;
    assert_eq!(state.list(&key("log")).unwrap(), &[Value::str("ok")]);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_failing_sibling_when_run_then_the_checkpoint_holds_the_state_before_the_superstep()
{
    let calls = Arc::new(AtomicUsize::new(0));
    let failing = Arc::new(AtomicBool::new(true));
    let graph = siblings(counted_log("ok", &calls), failing_while(&failing));
    let failure = first_run(&graph).await;
    let mut before = base_state();
    before
        .apply_batch(&[Update::Set {
            key: key("count"),
            value: Value::int(1),
        }])
        .unwrap();
    assert_eq!(failure.checkpoint.state, before);
    assert_eq!(
        failure.checkpoint.cursor.active,
        vec![nid("ok"), nid("bad")]
    );
    let pending: Vec<String> = failure
        .checkpoint
        .pending
        .iter()
        .map(|(occurrence, _)| occurrence.to_string())
        .collect();
    assert_eq!(pending, vec!["ok"]);
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_map_that_finished_before_its_sibling_failed_when_resumed_then_the_map_is_skipped_as_a_whole()
 {
    let counted = Arc::new(Counted::default());
    let map = Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(CountedBody {
            counted: counted.clone(),
            item: key("item"),
            outs: key("outs"),
            log: key("log"),
        }),
        max_concurrency: None,
    };
    let failing = Arc::new(AtomicBool::new(true));
    let graph = siblings(map, failing_while(&failing));
    let mut state = base_state();
    state
        .apply_batch(&[Update::Set {
            key: key("items"),
            value: Value::list(vec![Value::str("a"), Value::str("b")]),
        }])
        .unwrap();
    let failure = first_run_from(&graph, state).await;
    assert!(
        failure
            .checkpoint
            .state
            .list(&key("outs"))
            .unwrap()
            .is_empty()
    );
    let pending: Vec<String> = failure
        .checkpoint
        .pending
        .iter()
        .map(|(occurrence, _)| occurrence.to_string())
        .collect();
    assert_eq!(pending, vec!["ok", "ok[0]", "ok[1]"]);

    failing.store(false, Ordering::SeqCst);
    let state = resume(&graph, failure.checkpoint).await;
    assert_eq!(
        state.list(&key("outs")).unwrap(),
        &[Value::str("A"), Value::str("B")]
    );
    assert!(counted.calls().values().all(|calls| *calls == 1));
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_cancel_while_a_sibling_runs_when_resumed_then_the_finished_node_is_not_run_again()
{
    let items = ["slow"];
    let gates = Gates::new(&items);
    let calls = Arc::new(AtomicUsize::new(0));
    let slow = GatedBody {
        gates: gates.clone(),
        item: key("item"),
        lists: vec![key("outs")],
    };
    let graph = siblings(counted_log("ok", &calls), slow);
    let mut state = base_state();
    state
        .apply_batch(&[Update::Set {
            key: key("item"),
            value: Value::str("slow"),
        }])
        .unwrap();
    let (sender, mut inbox) = channel();
    let (ctx, config) = (ctx(), config());
    let running = run(&graph, &config, state, None, &ctx, &mut inbox);
    let driver = async {
        gates.until(|g| g.started().len() == 1).await;
        sender.cancel();
    };
    let (outcome, ()) = join(running, driver).await;
    let Ok(Outcome::Cancelled { checkpoint }) = outcome else {
        panic!("expected cancelled");
    };
    let pending: Vec<String> = checkpoint
        .pending
        .iter()
        .map(|(occurrence, _)| occurrence.to_string())
        .collect();
    assert_eq!(pending, vec!["ok"]);
    assert!(checkpoint.state.list(&key("log")).unwrap().is_empty());

    let state = resume(&graph, checkpoint).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(state.list(&key("log")).unwrap(), &[Value::str("ok")]);
    assert_eq!(state.list(&key("outs")).unwrap(), &[Value::str("slow")]);
    assert_eq!(gates.started(), vec!["slow", "slow"]);
}

#[tokio::test(flavor = "current_thread")]
async fn given_an_edge_that_fails_once_when_resumed_then_its_node_is_applied_once() {
    let calls = Arc::new(AtomicUsize::new(0));
    let failing = Arc::new(AtomicBool::new(true));
    let flag = failing.clone();
    let route = FnEdge::new(move |_s: &State, _c: &Config| {
        if flag.load(Ordering::SeqCst) {
            Ok(Vec::new())
        } else {
            Ok(vec![end("done")])
        }
    });
    let graph = GraphBuilder::new(schema())
        .entry(nid("ok"))
        .node(nid("ok"), counted_log("ok", &calls))
        .edge(nid("ok"), route)
        .build()
        .unwrap();
    let failure = first_run(&graph).await;
    assert!(
        failure
            .checkpoint
            .state
            .list(&key("log"))
            .unwrap()
            .is_empty()
    );
    failing.store(false, Ordering::SeqCst);
    let state = resume(&graph, failure.checkpoint).await;
    assert_eq!(state.list(&key("log")).unwrap(), &[Value::str("ok")]);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_node_whose_updates_are_refused_when_resumed_then_it_runs_again() {
    let refused = Arc::new(AtomicBool::new(true));
    let flag = refused.clone();
    let node = FnNode::new(
        move |_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            let target = if flag.swap(false, Ordering::SeqCst) {
                key("count")
            } else {
                key("log")
            };
            Box::pin(async move {
                Ok(vec![Update::Append {
                    key: target,
                    value: Value::str("x"),
                }])
            })
        },
    );
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), node)
        .edge(nid("a"), edge(vec![end("done")]))
        .build()
        .unwrap();
    let failure = first_run(&graph).await;
    assert!(failure.checkpoint.pending.is_empty());
    let state = resume(&graph, failure.checkpoint).await;
    assert_eq!(state.list(&key("log")).unwrap(), &[Value::str("x")]);
}
