use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::test_support::*;
use crate::graph::subgraph::{Input, Output, SubGraph};
use crate::graph::{Always, Context, FnNode, Graph, GraphBuilder, Map, NodeFuture, Target};
use crate::observe::{NoopObserver, Observer};
use crate::origin::Origin;
use crate::run::counted::{Counted, CountedBody};
use crate::run::{Outcome, PendingEntry, channel, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;

/// Splits `text` into two parts, maps `inner` over them with a counted body,
/// then joins the pieces into `summary`. Input `text`, output `summary`.
fn split_and_join(calls: &Arc<Counted>) -> Arc<Graph> {
    let schema = Schema::builder()
        .state(key("text"), Kind::Str)
        .state(key("parts"), Kind::list(Kind::Str))
        .state(key("part"), Kind::Str)
        .state(key("pieces"), Kind::list(Kind::Str))
        .state(key("trail"), Kind::list(Kind::Str))
        .state(key("summary"), Kind::Str)
        .build();
    let split = FnNode::new(|s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
        let text = s.str(&key("text")).map(str::to_owned);
        Box::pin(async move {
            let text = text?;
            Ok(vec![Update::Set {
                key: key("parts"),
                value: Value::list(vec![
                    Value::str(format!("{text}1")),
                    Value::str(format!("{text}2")),
                ]),
            }])
        })
    });
    let join = FnNode::new(|s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
        let pieces: Result<Vec<String>, _> = s.list(&key("pieces")).map(|values| {
            values
                .iter()
                .filter_map(|value| match value {
                    Value::Str(text) => Some(text.clone()),
                    _ => None,
                })
                .collect()
        });
        Box::pin(async move {
            Ok(vec![Update::Set {
                key: key("summary"),
                value: Value::str(pieces?.join(",")),
            }])
        })
    });
    let inner = Map {
        list: key("parts"),
        item: key("part"),
        body: Box::new(CountedBody {
            counted: calls.clone(),
            item: key("part"),
            outs: key("pieces"),
            log: key("trail"),
        }),
        max_concurrency: None,
        on_item_failure: crate::graph::ItemFailure::Finish,
    };
    let graph = GraphBuilder::new(schema)
        .entry(nid("split"))
        .node(nid("split"), split)
        .map(nid("inner"), inner)
        .node(nid("join"), join)
        .edge(nid("split"), Always(Target::Node(nid("inner"))))
        .edge(nid("inner"), Always(Target::Node(nid("join"))))
        .edge(nid("join"), Always(end("done")))
        .input(key("text"))
        .output(key("summary"))
        .build()
        .unwrap();
    Arc::new(graph)
}

fn each_item(calls: &Arc<Counted>) -> Graph {
    let body = SubGraph::call(split_and_join(calls))
        .input(key("text"), Input::From(key("item")))
        .output(key("summary"), Output::Append(key("results")));
    let map = Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(body),
        max_concurrency: None,
        on_item_failure: crate::graph::ItemFailure::Finish,
    };
    GraphBuilder::new(parent_schema())
        .entry(nid("each"))
        .map(nid("each"), map)
        .edge(nid("each"), Always(end("done")))
        .input(key("items"))
        .build()
        .unwrap()
}

fn start(graph: &Graph) -> State {
    let items = Value::list(vec![Value::str("x"), Value::str("y")]);
    graph.start_state([(key("items"), items)]).unwrap()
}

#[derive(Default)]
struct Records(Mutex<Vec<String>>);

impl Observer for Records {
    fn recorded(&self, origin: &Origin, _entry: &PendingEntry) {
        self.0.lock().unwrap().push(origin.occurrence.to_string());
    }
}

fn summaries() -> Vec<Value> {
    vec![Value::str("X1,X2"), Value::str("Y1,Y2")]
}

#[tokio::test]
async fn given_items_that_call_a_graph_with_an_inner_map_when_run_then_only_the_outer_run_records()
{
    let graph = each_item(&Arc::new(Counted::default()));
    let records = Arc::new(Records::default());
    let outcome = run_parent(&graph, start(&graph), &parent_config(1), records.clone()).await;
    let Ok(Outcome::Finished { state, .. }) = outcome else {
        panic!("expected finished");
    };
    assert_eq!(state.list(&key("results")).unwrap(), summaries().as_slice());
    let mut seen = records.0.lock().unwrap().clone();
    seen.sort();
    assert_eq!(seen, vec!["each[0]", "each[1]"]);
}

#[tokio::test]
async fn given_an_inner_item_failing_inside_a_call_when_resumed_then_finished_outer_items_are_skipped_and_the_call_restarts_whole()
 {
    let calls = Counted::failing_on("y2");
    let graph = each_item(&calls);
    let failure = run_parent(
        &graph,
        start(&graph),
        &parent_config(1),
        Arc::new(NoopObserver),
    )
    .await
    .err()
    .unwrap();
    let pending: Vec<String> = failure
        .checkpoint
        .pending
        .iter()
        .map(|(key, _)| key.to_string())
        .collect();
    assert_eq!(pending, vec!["each[0]"]);

    calls.heal();
    let checkpoint = failure.checkpoint;
    let (_sender, mut inbox) = channel();
    let ctx = context(Arc::new(NoopObserver)).with_pending(checkpoint.pending);
    let outcome = run(
        &graph,
        &parent_config(1),
        checkpoint.state,
        Some(checkpoint.cursor),
        &ctx,
        &mut inbox,
    )
    .await;
    let Ok(Outcome::Finished { state, .. }) = outcome else {
        panic!("expected finished");
    };
    assert_eq!(state.list(&key("results")).unwrap(), summaries().as_slice());
    let expected: BTreeMap<String, usize> = [("x1", 1), ("x2", 1), ("y1", 2), ("y2", 2)]
        .into_iter()
        .map(|(item, count)| (item.to_owned(), count))
        .collect();
    assert_eq!(calls.calls(), expected);
}
