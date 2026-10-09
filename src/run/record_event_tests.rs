//! Which records the observer sees: map items, and nodes that share their
//! superstep with siblings. A node alone in its superstep is recorded
//! silently.

use std::sync::{Arc, Mutex};

use crate::graph::{Context, FnNode, Graph, GraphBuilder, ItemFailure, Map, NodeFuture};
use crate::observe::Observer;
use crate::origin::Origin;
use crate::run::PendingEntry;
use crate::run::inbox::{Sender, channel};
use crate::run::outcome::Outcome;
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::{Config, State, Value};
use crate::update::Update;

#[derive(Default)]
struct Records(Mutex<Vec<String>>);

impl Records {
    fn seen(&self) -> Vec<String> {
        let mut seen = self.0.lock().unwrap().clone();
        seen.sort();
        seen
    }
}

impl Observer for Records {
    fn recorded(&self, origin: &Origin, _entry: &PendingEntry) {
        self.0.lock().unwrap().push(origin.occurrence.to_string());
    }
}

async fn observed(graph: &Graph, state: State) -> (Arc<Records>, Outcome) {
    let records = Arc::new(Records::default());
    let ctx = crate::testkit::context(records.clone());
    let (_sender, mut inbox) = channel();
    let outcome = run(graph, &config(), state, None, &ctx, &mut inbox)
        .await
        .map_err(|failure| failure.error)
        .unwrap();
    (records, outcome)
}

#[tokio::test]
async fn given_nodes_alone_in_their_supersteps_when_run_then_no_record_is_observed() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), log_node("a"))
        .node(nid("b"), log_node("b"))
        .edge(nid("a"), edge(vec![to("b")]))
        .edge(nid("b"), edge(vec![end("done")]))
        .build()
        .unwrap();
    let (records, outcome) = observed(&graph, base_state()).await;
    assert!(matches!(outcome, Outcome::Finished { .. }));
    assert!(records.seen().is_empty());
}

#[tokio::test]
async fn given_two_nodes_in_one_superstep_when_run_then_each_record_is_observed() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), noop_node())
        .node(nid("left"), log_node("left"))
        .node(nid("right"), log_node("right"))
        .edge(nid("start"), edge(vec![to("left"), to("right")]))
        .edge(nid("left"), edge(vec![end("done")]))
        .edge(nid("right"), edge(vec![end("done")]))
        .build()
        .unwrap();
    let (records, _) = observed(&graph, base_state()).await;
    assert_eq!(records.seen(), vec!["left", "right"]);
}

#[tokio::test]
async fn given_a_map_alone_in_its_superstep_when_run_then_its_items_are_observed_not_the_map() {
    let map = Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(FnNode::new(
            |s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
                let item = s.str(&key("item")).map(str::to_owned);
                Box::pin(async move {
                    Ok(vec![Update::Append {
                        key: key("outs"),
                        value: Value::str(item?),
                    }])
                })
            },
        )),
        max_concurrency: None,
        on_item_failure: ItemFailure::Finish,
    };
    let graph = GraphBuilder::new(schema())
        .entry(nid("m"))
        .map(nid("m"), map)
        .edge(nid("m"), edge(vec![end("done")]))
        .build()
        .unwrap();
    let mut state = base_state();
    state
        .apply_batch(&[Update::Set {
            key: key("items"),
            value: Value::list(vec![Value::str("a"), Value::str("b")]),
        }])
        .unwrap();
    let (records, _) = observed(&graph, state).await;
    assert_eq!(records.seen(), vec!["m[0]", "m[1]"]);
}

#[tokio::test]
async fn given_a_cancel_right_after_a_lone_node_finished_when_resumed_then_its_result_is_kept() {
    let (sender, mut inbox) = channel();
    let cancel: Arc<Mutex<Option<Sender>>> = Arc::new(Mutex::new(Some(sender)));
    let trigger = cancel.clone();
    let node = FnNode::new(
        move |_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            if let Some(sender) = trigger.lock().unwrap().take() {
                sender.cancel();
            }
            Box::pin(async {
                Ok(vec![Update::Append {
                    key: key("log"),
                    value: Value::str("once"),
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
    let records = Arc::new(Records::default());
    let ctx = crate::testkit::context(records.clone());
    let outcome = run(&graph, &config(), base_state(), None, &ctx, &mut inbox).await;
    let Ok(Outcome::Cancelled { checkpoint }) = outcome else {
        panic!("expected cancelled");
    };
    assert!(records.seen().is_empty());
    assert!(checkpoint.pending.get(&"a".parse().unwrap()).is_some());

    let resumed = crate::testkit::context(records.clone()).with_pending(checkpoint.pending);
    let (_sender, mut inbox) = channel();
    let outcome = run(
        &graph,
        &config(),
        checkpoint.state,
        Some(checkpoint.cursor),
        &resumed,
        &mut inbox,
    )
    .await;
    let Ok(Outcome::Finished { state, .. }) = outcome else {
        panic!("expected finished");
    };
    assert_eq!(state.list(&key("log")).unwrap(), &[Value::str("once")]);
}
