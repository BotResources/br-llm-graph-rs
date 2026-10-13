//! Pending entries of the run being resumed: items of a map directly inside a
//! map, and top-level items checked against their witness.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::graph::{Context, FnNode, Graph, GraphBuilder, ItemFailure, Map, Node, NodeFuture};
use crate::observe::NoopObserver;
use crate::run::counted::{Counted, CountedBody};
use crate::run::inbox::channel;
use crate::run::outcome::{Outcome, RunFailure};
use crate::run::runner::run;
use crate::run::test_support::{edge, end, key, nid};
use crate::run::{PendingEntry, PendingWrites};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;

fn texts(values: &[&str]) -> Value {
    Value::list(values.iter().map(|value| Value::str(*value)).collect())
}

fn schema() -> Schema {
    Schema::builder()
        .state(key("outers"), Kind::list(Kind::Str))
        .state(key("outer"), Kind::Str)
        .state(key("inners"), Kind::list(Kind::Str))
        .state(key("inner"), Kind::Str)
        .state(key("outs"), Kind::list(Kind::Str))
        .state(key("log"), Kind::list(Kind::Str))
        .build()
}

fn map_over(list: &str, item: &str, body: impl Node + 'static) -> Map {
    Map {
        list: key(list),
        item: key(item),
        body: Box::new(body),
        max_concurrency: None,
        on_item_failure: ItemFailure::Finish,
    }
}

fn single_map(map: Map) -> Graph {
    GraphBuilder::new(schema())
        .entry(nid("m"))
        .map(nid("m"), map)
        .edge(nid("m"), edge(vec![end("done")]))
        .build()
        .unwrap()
}

fn start(graph: &Graph, outers: &[&str], inners: &[&str]) -> State {
    graph
        .start_state(Vec::new())
        .and_then(|mut state| {
            state.apply_batch(&[
                Update::Set {
                    key: key("outers"),
                    value: texts(outers),
                },
                Update::Set {
                    key: key("inners"),
                    value: texts(inners),
                },
            ])?;
            Ok(state)
        })
        .unwrap()
}

async fn run_with(
    graph: &Graph,
    state: State,
    pending: PendingWrites,
) -> Result<Outcome, RunFailure> {
    let config = Config::new(graph.schema(), BTreeMap::new()).unwrap();
    let ctx = crate::testkit::context(Arc::new(NoopObserver)).with_pending(pending);
    let (_sender, mut inbox) = channel();
    run(graph, &config, state, None, &ctx, &mut inbox).await
}

fn finished(outcome: Result<Outcome, RunFailure>) -> State {
    match outcome {
        Ok(Outcome::Finished { state, .. }) => state,
        Ok(_) => panic!("expected finished"),
        Err(failure) => panic!("expected finished, got {}", failure.error),
    }
}

#[tokio::test]
async fn given_a_map_directly_in_a_map_when_resumed_then_finished_inner_items_are_skipped() {
    let calls: Arc<Mutex<BTreeMap<String, usize>>> = Arc::default();
    let failing = Arc::new(Mutex::new(Some("y2".to_owned())));
    let (counter, fail) = (calls.clone(), failing.clone());
    let pair = FnNode::new(
        move |s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            let name = s
                .str(&key("outer"))
                .and_then(|outer| Ok(format!("{outer}{}", s.str(&key("inner"))?)));
            let (counter, fail) = (counter.clone(), fail.clone());
            Box::pin(async move {
                let name = name?;
                *counter.lock().unwrap().entry(name.clone()).or_default() += 1;
                if fail.lock().unwrap().as_deref() == Some(name.as_str()) {
                    return Err(format!("{name} failed").into());
                }
                Ok(vec![Update::Append {
                    key: key("outs"),
                    value: Value::str(name),
                }])
            })
        },
    );
    let graph = single_map(map_over(
        "outers",
        "outer",
        map_over("inners", "inner", pair),
    ));
    let failure = run_with(
        &graph,
        start(&graph, &["x", "y"], &["1", "2"]),
        PendingWrites::new(),
    )
    .await
    .err()
    .unwrap();
    let checkpoint = failure.checkpoint;
    let pending: Vec<String> = checkpoint
        .pending
        .iter()
        .map(|(key, _)| key.to_string())
        .collect();
    assert_eq!(pending, vec!["m[0]", "m[0]/m[0]", "m[0]/m[1]", "m[1]/m[0]"]);

    *failing.lock().unwrap() = None;
    let config = Config::new(graph.schema(), BTreeMap::new()).unwrap();
    let ctx = crate::testkit::context(Arc::new(NoopObserver)).with_pending(checkpoint.pending);
    let (_sender, mut inbox) = channel();
    let state = finished(
        run(
            &graph,
            &config,
            checkpoint.state,
            Some(checkpoint.cursor),
            &ctx,
            &mut inbox,
        )
        .await,
    );
    assert_eq!(
        state.list(&key("outs")).unwrap(),
        &["x1", "x2", "y1", "y2"].map(Value::str)
    );
    let expected: BTreeMap<String, usize> = [("x1", 1), ("x2", 1), ("y1", 1), ("y2", 2)]
        .into_iter()
        .map(|(name, count)| (name.to_owned(), count))
        .collect();
    assert_eq!(*calls.lock().unwrap(), expected);
}

fn counted_items(counted: &Arc<Counted>) -> Graph {
    single_map(map_over(
        "outers",
        "outer",
        CountedBody {
            counted: counted.clone(),
            item: key("outer"),
            outs: key("outs"),
            log: key("log"),
        },
    ))
}

fn recorded(witness: &str, out: &str) -> PendingWrites {
    let mut pending = PendingWrites::new();
    pending.insert(
        "m[0]".parse().unwrap(),
        PendingEntry::witnessed(
            Value::str(witness),
            vec![
                Update::Append {
                    key: key("outs"),
                    value: Value::str(out),
                },
                Update::Append {
                    key: key("log"),
                    value: Value::str(format!("{witness}@log")),
                },
            ],
        ),
    );
    pending
}

#[tokio::test]
async fn given_an_item_recorded_on_the_same_value_when_run_then_its_appends_are_used() {
    let counted = Arc::new(Counted::default());
    let graph = counted_items(&counted);
    let state = finished(
        run_with(
            &graph,
            start(&graph, &["a", "b"], &[]),
            recorded("a", "FROM RECORD"),
        )
        .await,
    );
    assert_eq!(
        state.list(&key("outs")).unwrap(),
        &["FROM RECORD", "B"].map(Value::str)
    );
    assert_eq!(counted.calls().get("a"), None);
}

#[tokio::test]
async fn given_an_item_recorded_on_another_value_when_run_then_the_record_is_ignored() {
    let counted = Arc::new(Counted::default());
    let graph = counted_items(&counted);
    let state = finished(
        run_with(
            &graph,
            start(&graph, &["a", "b"], &[]),
            recorded("z", "STALE"),
        )
        .await,
    );
    assert_eq!(
        state.list(&key("outs")).unwrap(),
        &["A", "B"].map(Value::str)
    );
    assert_eq!(counted.calls().get("a"), Some(&1));
}
