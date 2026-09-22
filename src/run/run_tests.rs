use std::sync::Arc;

use crate::graph::{Always, Context, GraphBuilder, Map};
use crate::observe::NoopObserver;
use crate::run::inbox::channel;
use crate::run::outcome::Outcome;
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::Value;
use crate::testkit::{Recorder, context};

fn ctx() -> Context {
    Context::new(
        Arc::new(NoopObserver),
        Arc::new(crate::testkit::SeqIds::new()),
    )
}

#[tokio::test]
async fn given_fan_out_then_join_when_run_then_join_runs_once() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), noop_node())
        .node(nid("a"), log_node("a"))
        .node(nid("b"), log_node("b"))
        .join(nid("j"), log_node("j"))
        .edge(nid("start"), edge(vec![to("a"), to("b")]))
        .edge(nid("a"), Always(to("j")))
        .edge(nid("b"), Always(to("j")))
        .edge(nid("j"), edge(vec![end("done")]))
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let outcome = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Finished { state, end } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(end.as_str(), "done");
    let log: Vec<&str> = state
        .list(&key("log"))
        .unwrap()
        .iter()
        .map(|v| match v {
            Value::Str(s) => s.as_str(),
            _ => "?",
        })
        .collect();
    assert_eq!(log, vec!["a", "b", "j"]);
}

#[tokio::test]
async fn given_unequal_branches_when_run_then_join_still_runs_once() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), noop_node())
        .node(nid("a"), log_node("a"))
        .node(nid("b"), log_node("b"))
        .node(nid("c"), log_node("c"))
        .join(nid("j"), log_node("j"))
        .edge(nid("start"), edge(vec![to("a"), to("b")]))
        .edge(nid("a"), Always(to("j")))
        .edge(nid("b"), Always(to("c")))
        .edge(nid("c"), Always(to("j")))
        .edge(nid("j"), edge(vec![end("done")]))
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let outcome = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    let joins = state
        .list(&key("log"))
        .unwrap()
        .iter()
        .filter(|v| matches!(v, Value::Str(s) if s == "j"))
        .count();
    assert_eq!(joins, 1);
}

async fn run_map_over(items: Vec<&str>) -> Vec<String> {
    let map = Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(item_to_out_node()),
        output: key("out"),
        results: key("outs"),
    };
    let graph = GraphBuilder::new(schema())
        .entry(nid("m"))
        .map(nid("m"), map)
        .edge(nid("m"), edge(vec![end("done")]))
        .build()
        .unwrap();
    let mut state = base_state();
    state
        .apply_batch(&[crate::update::Update::Set {
            key: key("items"),
            value: Value::list(items.iter().map(|s| Value::str(*s)).collect()),
        }])
        .unwrap();
    let (_sender, mut inbox) = channel();
    let outcome = run(&graph, &config(), state, None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    state
        .list(&key("outs"))
        .unwrap()
        .iter()
        .map(|v| match v {
            Value::Str(s) => s.clone(),
            _ => String::new(),
        })
        .collect()
}

#[tokio::test]
async fn given_map_over_three_items_when_run_then_three_outputs() {
    let outs = run_map_over(vec!["a", "b", "c"]).await;
    assert_eq!(outs, vec!["A", "B", "C"]);
}

#[tokio::test]
async fn given_map_over_empty_list_when_run_then_no_outputs() {
    let outs = run_map_over(Vec::new()).await;
    assert!(outs.is_empty());
}

#[tokio::test]
async fn given_two_distinct_ends_in_one_superstep_when_run_then_ambiguous() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), noop_node())
        .node(nid("a"), noop_node())
        .node(nid("b"), noop_node())
        .edge(nid("start"), edge(vec![to("a"), to("b")]))
        .edge(nid("a"), edge(vec![end("x")]))
        .edge(nid("b"), edge(vec![end("y")]))
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure.error,
        crate::error::GraphError::AmbiguousEnd { .. }
    ));
}

#[tokio::test]
async fn given_failing_node_beside_success_when_run_then_success_kept_failure_reported() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), noop_node())
        .node(nid("a"), set_count_node(5))
        .node(nid("b"), failing_node())
        .edge(nid("start"), edge(vec![to("a"), to("b")]))
        .edge(nid("a"), Always(end("done")))
        .edge(nid("b"), Always(end("done")))
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure.error,
        crate::error::GraphError::NodeFailed { .. }
    ));
    assert_eq!(failure.checkpoint.state.int(&key("count")).unwrap(), 5);
}

#[tokio::test]
async fn given_panicking_node_when_run_then_node_failed_with_state_intact() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("p"))
        .node(nid("p"), panicking_node())
        .edge(nid("p"), Always(end("done")))
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
        .await
        .err()
        .unwrap();
    match failure.error {
        crate::error::GraphError::NodeFailed { source, .. } => {
            assert!(matches!(source, crate::error::NodeFault::Panic(_)));
        }
        other => panic!("expected NodeFailed, got {other}"),
    }
    assert_eq!(failure.checkpoint.state.int(&key("count")).unwrap(), 0);
}

#[tokio::test]
async fn given_two_nodes_set_same_key_when_run_then_declaration_order_wins() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), noop_node())
        .node(nid("a"), set_count_node(1))
        .node(nid("b"), set_count_node(2))
        .edge(nid("start"), edge(vec![to("a"), to("b")]))
        .edge(nid("a"), Always(end("done")))
        .edge(nid("b"), Always(end("done")))
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let outcome = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(state.int(&key("count")).unwrap(), 2);
}

#[tokio::test]
async fn given_end_mixed_with_node_target_when_run_then_only_final_end_counts() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), noop_node())
        .node(nid("a"), noop_node())
        .node(nid("b"), noop_node())
        .node(nid("c"), noop_node())
        .edge(nid("start"), edge(vec![to("a"), to("b")]))
        .edge(nid("a"), edge(vec![end("early")]))
        .edge(nid("b"), Always(to("c")))
        .edge(nid("c"), edge(vec![end("late")]))
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let outcome = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Finished { end, .. } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(end.as_str(), "late");
}

#[tokio::test]
async fn given_edge_returning_no_target_when_run_then_empty_edge_error() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), set_count_node(3))
        .edge(nid("a"), edge(Vec::new()))
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure.error,
        crate::error::GraphError::EmptyEdge { .. }
    ));
    assert_eq!(failure.checkpoint.state.int(&key("count")).unwrap(), 3);
}

#[tokio::test]
async fn given_observer_when_run_then_started_before_finished_before_checkpoint() {
    let recorder = Arc::new(Recorder::default());
    let ctx = context(recorder.clone());
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), log_node("a"))
        .edge(nid("a"), Always(end("done")))
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    run(&graph, &config(), base_state(), None, &ctx, &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let lines = recorder.lines();
    let started = lines.iter().position(|l| l == "started a").unwrap();
    let finished = lines.iter().position(|l| l == "finished a").unwrap();
    let applied = lines.iter().position(|l| l.starts_with("applied")).unwrap();
    let checkpoint = lines
        .iter()
        .position(|l| l.starts_with("checkpoint"))
        .unwrap();
    let run_done = lines.iter().position(|l| l == "finished run done").unwrap();
    assert!(started < finished);
    assert!(finished < applied);
    assert!(applied < checkpoint);
    assert!(checkpoint < run_done);
}
