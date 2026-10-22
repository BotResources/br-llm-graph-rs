//! The node finished event says how each node of a superstep ended: with its
//! updates, returned or reused, with an error or with a panic. A cancel that
//! drops the nodes while they run gives no event; a cancel received just as
//! the superstep completes comes after it.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::error::{GraphError, NodeFault};
use crate::graph::{Always, Context, FnNode, GraphBuilder, NodeError, NodeFuture};
use crate::observe::NodeOutcome;
use crate::run::control::control;
use crate::run::outcome::Outcome;
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::{Config, State, Value};
use crate::testkit::Recorder;
use crate::update::Update;

#[tokio::test]
async fn given_a_superstep_where_one_node_succeeds_one_errs_and_one_panics_when_run_then_each_says_how_it_ended()
 {
    let graph = GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), noop_node())
        .node(nid("ok"), log_node("ok"))
        .node(nid("err"), failing_node())
        .node(nid("boom"), panicking_node())
        .edge(nid("start"), edge(vec![to("ok"), to("err"), to("boom")]))
        .edge(nid("ok"), Always(end("done")))
        .edge(nid("err"), Always(end("done")))
        .edge(nid("boom"), Always(end("done")))
        .build()
        .unwrap();
    let finishes = Arc::new(Finishes::default());
    let ctx = crate::testkit::context(finishes.clone());
    let (_sender, mut channel) = control();
    let failure = run(&graph, &config(), base_state(), None, &ctx, &mut channel)
        .await
        .err()
        .unwrap();
    assert!(matches!(failure.error, GraphError::NodeFailed { .. }));
    assert_eq!(
        finishes.seen(),
        finished(&[
            ("|start", NodeOutcome::Updates),
            ("|ok", NodeOutcome::Updates),
            ("|err", NodeOutcome::Error),
            ("|boom", NodeOutcome::Panic),
        ])
    );
}

#[tokio::test]
async fn given_a_node_whose_updates_are_refused_when_run_then_it_finished_with_its_updates() {
    let wrong_kind = FnNode::new(|_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
        Box::pin(async {
            Ok(vec![Update::Set {
                key: key("count"),
                value: Value::str("three"),
            }])
        })
    });
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), wrong_kind)
        .edge(nid("a"), Always(end("done")))
        .build()
        .unwrap();
    let finishes = Arc::new(Finishes::default());
    let ctx = crate::testkit::context(finishes.clone());
    let (_sender, mut channel) = control();
    let failure = run(&graph, &config(), base_state(), None, &ctx, &mut channel)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure.error,
        GraphError::NodeFailed {
            source: NodeFault::Refused(_),
            ..
        }
    ));
    assert_eq!(finishes.seen(), finished(&[("|a", NodeOutcome::Updates)]));
}

#[tokio::test]
async fn given_a_node_recorded_before_a_failure_when_resumed_then_it_finished_with_its_recorded_updates()
 {
    let runs = Arc::new(AtomicUsize::new(0));
    let counted = runs.clone();
    let steady = FnNode::new(
        move |_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            counted.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Ok(vec![Update::Append {
                    key: key("log"),
                    value: Value::str("steady"),
                }])
            })
        },
    );
    let attempts = AtomicUsize::new(0);
    let flaky = FnNode::new(
        move |_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            let first = attempts.fetch_add(1, Ordering::SeqCst) == 0;
            Box::pin(async move {
                if first {
                    Err::<Vec<Update>, NodeError>("not yet".into())
                } else {
                    Ok(Vec::new())
                }
            })
        },
    );
    let graph = GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), noop_node())
        .node(nid("steady"), steady)
        .node(nid("flaky"), flaky)
        .edge(nid("start"), edge(vec![to("steady"), to("flaky")]))
        .edge(nid("steady"), Always(end("done")))
        .edge(nid("flaky"), Always(end("done")))
        .build()
        .unwrap();
    let first = Arc::new(Finishes::default());
    let (_sender, mut channel) = control();
    let ctx = crate::testkit::context(first.clone());
    let failure = run(&graph, &config(), base_state(), None, &ctx, &mut channel)
        .await
        .err()
        .unwrap();
    assert_eq!(
        first.seen(),
        finished(&[
            ("|start", NodeOutcome::Updates),
            ("|steady", NodeOutcome::Updates),
            ("|flaky", NodeOutcome::Error),
        ])
    );

    let resumed = Arc::new(Finishes::default());
    let checkpoint = failure.checkpoint;
    let ctx = crate::testkit::context(resumed.clone()).with_pending(checkpoint.pending);
    let outcome = run(
        &graph,
        &config(),
        checkpoint.state,
        Some(checkpoint.cursor),
        &ctx,
        &mut channel,
    )
    .await;
    let Ok(Outcome::Finished { state, .. }) = outcome else {
        panic!("expected finished");
    };
    assert_eq!(runs.load(Ordering::SeqCst), 1, "the recorded node ran once");
    assert_eq!(state.list(&key("log")).unwrap(), &[Value::str("steady")]);
    assert_eq!(
        resumed.seen(),
        finished(&[
            ("|steady", NodeOutcome::Updates),
            ("|flaky", NodeOutcome::Updates),
        ])
    );
}

#[tokio::test]
async fn given_a_cancel_that_drops_the_running_node_when_run_then_no_node_finished_is_observed() {
    let waits = FnNode::new(|_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
        Box::pin(async {
            futures_util::future::pending::<()>().await;
            Ok(Vec::new())
        })
    });
    let graph = GraphBuilder::new(schema())
        .entry(nid("work"))
        .node(nid("work"), waits)
        .edge(nid("work"), Always(end("done")))
        .build()
        .unwrap();
    let recorder = Arc::new(Recorder::default());
    let ctx = crate::testkit::context(recorder.clone());
    let (sender, mut channel) = control();
    sender.cancel();
    let outcome = run(&graph, &config(), base_state(), None, &ctx, &mut channel).await;
    assert!(matches!(outcome, Ok(Outcome::Cancelled { .. })));
    assert_eq!(recorder.lines(), vec!["started work"]);
}

#[tokio::test]
async fn given_a_cancel_received_as_the_superstep_completes_when_run_then_it_comes_after_node_finished()
 {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), log_node("a"))
        .edge(nid("a"), Always(end("done")))
        .build()
        .unwrap();
    let recorder = Arc::new(Recorder::default());
    let ctx = crate::testkit::context(recorder.clone());
    let (sender, mut channel) = control();
    sender.cancel();
    let outcome = run(&graph, &config(), base_state(), None, &ctx, &mut channel).await;
    assert!(matches!(outcome, Ok(Outcome::Cancelled { .. })));
    assert_eq!(recorder.lines(), vec!["started a", "finished a Updates"]);
}

#[tokio::test]
async fn given_a_superstep_of_two_nodes_when_observed_then_each_finishes_after_every_start_and_before_the_applied_updates()
 {
    let graph = GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), noop_node())
        .node(nid("left"), log_node("left"))
        .node(nid("right"), log_node("right"))
        .edge(nid("start"), edge(vec![to("left"), to("right")]))
        .edge(nid("left"), Always(end("done")))
        .edge(nid("right"), Always(end("done")))
        .build()
        .unwrap();
    let recorder = Arc::new(Recorder::default());
    let ctx = crate::testkit::context(recorder.clone());
    let (_sender, mut channel) = control();
    let outcome = run(&graph, &config(), base_state(), None, &ctx, &mut channel).await;
    assert!(matches!(outcome, Ok(Outcome::Finished { .. })));
    assert_eq!(
        recorder.lines(),
        vec![
            "started start",
            "finished start Updates",
            "checkpoint [left,right]",
            "started left",
            "started right",
            "finished left Updates",
            "finished right Updates",
            "applied append \"left\" to log",
            "applied append \"right\" to log",
            "checkpoint []",
            "finished run done",
        ]
    );
}
