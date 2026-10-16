//! Cancel through the control channel: the superstep is dropped and not
//! applied, the inputs held so far are kept, and a resume runs it again.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::graph::{Always, GraphBuilder};
use crate::graph::{FnNode, NodeFuture};
use crate::run::control::control;
use crate::run::outcome::Outcome;
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::{Config, State, Value};
use crate::update::Update;

#[tokio::test]
async fn given_cancel_mid_superstep_when_run_then_state_untouched_and_reruns_on_resume() {
    let flag = Arc::new(AtomicUsize::new(0));
    let node_flag = flag.clone();
    let graph = GraphBuilder::new(schema())
        .entry(nid("work"))
        .node(
            nid("work"),
            FnNode::new(move |_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
                let flag = node_flag.clone();
                Box::pin(async move {
                    if flag.fetch_add(1, Ordering::SeqCst) == 0 {
                        futures_util::future::pending::<()>().await;
                    }
                    Ok(vec![Update::Set {
                        key: key("count"),
                        value: Value::int(1),
                    }])
                })
            }),
        )
        .edge(nid("work"), Always(end("done")))
        .build()
        .unwrap();

    let (sender, mut channel) = control();
    sender.cancel();
    let cancelled = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Cancelled { checkpoint } = cancelled else {
        panic!("expected cancelled");
    };
    assert_eq!(checkpoint.state.int(&key("count")).unwrap(), 0);
    assert_eq!(
        checkpoint
            .cursor
            .active
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>(),
        vec!["work"]
    );

    let resumed = run(
        &graph,
        &config(),
        checkpoint.state,
        Some(checkpoint.cursor),
        &ctx(),
        &mut channel,
    )
    .await
    .map_err(|f| f.error)
    .unwrap();
    let Outcome::Finished { state, end } = resumed else {
        panic!("expected finished");
    };
    assert_eq!(end.as_str(), "done");
    assert_eq!(state.int(&key("count")).unwrap(), 1);
}

#[tokio::test]
async fn given_cancel_racing_a_fast_superstep_when_run_then_state_untouched_and_reruns_once() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), log_node("a"))
        .edge(nid("a"), Always(end("done")))
        .build()
        .unwrap();
    let (sender, mut channel) = control();
    sender.cancel();
    let cancelled = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Cancelled { checkpoint } = cancelled else {
        panic!("expected cancelled");
    };
    assert!(checkpoint.state.list(&key("log")).unwrap().is_empty());
    assert_eq!(
        checkpoint
            .cursor
            .active
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>(),
        vec!["a"]
    );

    let resumed = run(
        &graph,
        &config(),
        checkpoint.state,
        Some(checkpoint.cursor),
        &ctx(),
        &mut channel,
    )
    .await
    .map_err(|f| f.error)
    .unwrap();
    let Outcome::Finished { state, .. } = resumed else {
        panic!("expected finished");
    };
    let log: Vec<&str> = state
        .list(&key("log"))
        .unwrap()
        .iter()
        .map(|v| match v {
            Value::Str(s) => s.as_str(),
            _ => "?",
        })
        .collect();
    assert_eq!(log, vec!["a"]);
}

#[tokio::test]
async fn given_input_and_cancel_same_superstep_when_run_then_cancelled_with_input_kept() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("work"))
        .node(
            nid("work"),
            FnNode::new(|_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
                Box::pin(async {
                    futures_util::future::pending::<()>().await;
                    Ok(Vec::new())
                })
            }),
        )
        .edge(nid("work"), Always(end("done")))
        .build()
        .unwrap();
    let (sender, mut channel) = control();
    sender.send(key("chat"), input("mid-flight"));
    sender.cancel();
    let cancelled = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Cancelled { checkpoint } = cancelled else {
        panic!("expected cancelled");
    };
    assert_eq!(
        checkpoint
            .state
            .conversation(&key("chat"))
            .unwrap()
            .entries()
            .len(),
        1
    );
    assert_eq!(
        checkpoint
            .cursor
            .active
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>(),
        vec!["work"]
    );
}
