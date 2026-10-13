//! A called graph that failed restarts from its entry on resume: nothing
//! recorded inside it may be reused, since a loop may be on another round.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::test_support::*;
use crate::graph::subgraph::{Output, SubGraph};
use crate::graph::{Always, Context, FnEdge, FnNode, Graph, GraphBuilder, NodeFuture, Target};
use crate::observe::NoopObserver;
use crate::run::{Outcome, PendingEntry, PendingWrites, channel, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;

/// Two rounds: `tick` counts the round, then `a` (appends `a<round>` to
/// `seen`) and `b` run together. `b` fails in round 2 while `failing` holds.
fn two_rounds(failing: &Arc<AtomicBool>) -> Arc<Graph> {
    let schema = Schema::builder()
        .state(key("count"), Kind::Int)
        .state(key("seen"), Kind::list(Kind::Str))
        .build();
    let tick = FnNode::new(|s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
        let count = s.int(&key("count")).map(|count| count + 1);
        Box::pin(async move {
            Ok(vec![Update::Set {
                key: key("count"),
                value: Value::int(count?),
            }])
        })
    });
    let a = FnNode::new(|s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
        let count = s.int(&key("count"));
        Box::pin(async move {
            Ok(vec![Update::Append {
                key: key("seen"),
                value: Value::str(format!("a{}", count?)),
            }])
        })
    });
    let flag = failing.clone();
    let b = FnNode::new(
        move |s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            let fail =
                s.int(&key("count")).is_ok_and(|count| count == 2) && flag.load(Ordering::SeqCst);
            Box::pin(async move {
                if fail {
                    Err("the second round failed".into())
                } else {
                    Ok(Vec::new())
                }
            })
        },
    );
    let route = || {
        FnEdge::new(|s: &State, _c: &Config| {
            if s.int(&key("count"))? < 2 {
                Ok(vec![Target::Node(nid("tick"))])
            } else {
                Ok(vec![end("done")])
            }
        })
    };
    let fork = FnEdge::new(|_s: &State, _c: &Config| {
        Ok(vec![Target::Node(nid("a")), Target::Node(nid("b"))])
    });
    let graph = GraphBuilder::new(schema)
        .entry(nid("tick"))
        .node(nid("tick"), tick)
        .node(nid("a"), a)
        .node(nid("b"), b)
        .edge(nid("tick"), fork)
        .edge(nid("a"), route())
        .edge(nid("b"), route())
        .output(key("seen"))
        .build()
        .unwrap();
    Arc::new(graph)
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_called_loop_failing_in_its_second_round_when_resumed_then_each_round_runs_its_own_nodes()
 {
    let failing = Arc::new(AtomicBool::new(true));
    let schema = Schema::builder()
        .state(key("result"), Kind::list(Kind::Str))
        .build();
    let call = SubGraph::call(two_rounds(&failing)).output(key("seen"), Output::Set(key("result")));
    let caller = GraphBuilder::new(schema.clone())
        .entry(nid("call"))
        .subgraph(nid("call"), call)
        .edge(nid("call"), Always(end("done")))
        .build()
        .unwrap();
    let config = Config::new(&schema, BTreeMap::new()).unwrap();
    let ctx = context(Arc::new(NoopObserver));
    let state = caller.start_state(Vec::new()).unwrap();
    let (_sender, mut inbox) = channel();
    let failure = run(&caller, &config, state, None, &ctx, &mut inbox)
        .await
        .err()
        .unwrap();
    let checkpoint = failure.checkpoint;
    failing.store(false, Ordering::SeqCst);
    let (_sender, mut inbox) = channel();
    let resumed = ctx.with_pending(checkpoint.pending);
    let outcome = run(
        &caller,
        &config,
        checkpoint.state,
        Some(checkpoint.cursor),
        &resumed,
        &mut inbox,
    )
    .await;
    let Ok(Outcome::Finished { state, .. }) = outcome else {
        panic!("expected finished");
    };
    assert_eq!(
        state.list(&key("result")).unwrap(),
        &[Value::str("a1"), Value::str("a2")]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn given_stale_entries_below_a_call_when_it_runs_then_they_are_ignored_and_cleared() {
    let failing = Arc::new(AtomicBool::new(true));
    let schema = Schema::builder()
        .state(key("result"), Kind::list(Kind::Str))
        .build();
    let call = SubGraph::call(two_rounds(&failing)).output(key("seen"), Output::Set(key("result")));
    let caller = GraphBuilder::new(schema.clone())
        .entry(nid("call"))
        .subgraph(nid("call"), call)
        .edge(nid("call"), Always(end("done")))
        .build()
        .unwrap();
    let config = Config::new(&schema, BTreeMap::new()).unwrap();
    let mut stale = PendingWrites::new();
    stale.insert(
        "call/a".parse().unwrap(),
        PendingEntry::new(vec![Update::Append {
            key: key("seen"),
            value: Value::str("stale"),
        }]),
    );
    stale.insert(
        "call/tick".parse().unwrap(),
        PendingEntry::new(vec![Update::Set {
            key: key("count"),
            value: Value::int(9),
        }]),
    );
    let ctx = context(Arc::new(NoopObserver)).with_pending(stale.clone());
    let (_sender, mut inbox) = channel();
    let state = caller.start_state(Vec::new()).unwrap();
    let failure = run(&caller, &config, state, None, &ctx, &mut inbox)
        .await
        .err()
        .unwrap();
    assert!(failure.checkpoint.pending.is_empty());

    failing.store(false, Ordering::SeqCst);
    let mut pending = failure.checkpoint.pending;
    pending.merge(stale);
    let (_sender, mut inbox) = channel();
    let resumed = context(Arc::new(NoopObserver)).with_pending(pending);
    let outcome = run(
        &caller,
        &config,
        failure.checkpoint.state,
        Some(failure.checkpoint.cursor),
        &resumed,
        &mut inbox,
    )
    .await;
    let Ok(Outcome::Finished { state, .. }) = outcome else {
        panic!("expected finished");
    };
    assert_eq!(
        state.list(&key("result")).unwrap(),
        &[Value::str("a1"), Value::str("a2")]
    );
}
