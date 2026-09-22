use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_llm_messages::{Text, UserBlock, UserInput, UserSource};

use crate::graph::{Always, Context, GraphBuilder};
use crate::graph::{FnNode, NodeFuture};
use crate::observe::NoopObserver;
use crate::run::inbox::channel;
use crate::run::outcome::Outcome;
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::{Config, State, Value};
use crate::update::Update;

fn ctx() -> Context {
    Context::new(
        Arc::new(NoopObserver),
        Arc::new(crate::testkit::SeqIds::new()),
    )
}

fn input(body: &str) -> UserInput {
    UserInput::new(
        UserSource::Human,
        None,
        vec![UserBlock::text(Text::new(body).unwrap())],
    )
    .unwrap()
}

#[tokio::test]
async fn given_input_queued_when_run_then_drained_at_boundary() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .node(nid("b"), noop_node())
        .edge(nid("a"), Always(to("b")))
        .edge(nid("b"), Always(end("done")))
        .build()
        .unwrap();
    let (sender, mut inbox) = channel();
    sender.send(key("chat"), input("hello"));
    let outcome = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(state.conversation(&key("chat")).unwrap().entries().len(), 1);
}

#[tokio::test]
async fn given_pause_when_run_then_paused_at_boundary_without_losing_work() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), set_count_node(7))
        .node(nid("b"), noop_node())
        .edge(nid("a"), Always(to("b")))
        .edge(nid("b"), Always(end("done")))
        .build()
        .unwrap();
    let (sender, mut inbox) = channel();
    sender.pause();
    let outcome = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Paused { checkpoint } = outcome else {
        panic!("expected paused");
    };
    assert_eq!(checkpoint.state.int(&key("count")).unwrap(), 7);
    assert_eq!(
        checkpoint
            .cursor
            .active
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>(),
        vec!["b"]
    );
}

#[tokio::test]
async fn given_resume_without_pause_when_run_then_ignored() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .edge(nid("a"), Always(end("done")))
        .build()
        .unwrap();
    let (sender, mut inbox) = channel();
    sender.resume();
    let outcome = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    assert!(matches!(outcome, Outcome::Finished { .. }));
}

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

    let (sender, mut inbox) = channel();
    sender.cancel();
    let cancelled = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
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
        &mut inbox,
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
    let (sender, mut inbox) = channel();
    sender.cancel();
    let cancelled = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
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
        &mut inbox,
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
async fn given_input_and_pause_same_superstep_when_run_then_paused_with_input_applied() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .node(nid("b"), noop_node())
        .edge(nid("a"), Always(to("b")))
        .edge(nid("b"), Always(end("done")))
        .build()
        .unwrap();
    let (sender, mut inbox) = channel();
    sender.send(key("chat"), input("hello"));
    sender.pause();
    let outcome = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Paused { checkpoint } = outcome else {
        panic!("expected paused");
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
        vec!["b"]
    );
}

#[tokio::test]
async fn given_checkpoint_with_unknown_node_when_validated_then_refused() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .edge(nid("a"), Always(end("done")))
        .build()
        .unwrap();
    let cursor = crate::run::Cursor::new(vec![nid("ghost")], Vec::new());
    assert!(matches!(
        graph.validate_cursor(&cursor),
        Err(crate::error::GraphError::UnknownNode { .. })
    ));
}
