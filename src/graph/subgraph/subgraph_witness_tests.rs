//! A called graph restarted after a failure may list its items differently.
//! It restarts whole: nothing recorded inside it is kept or reused.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::test_support::*;
use crate::graph::subgraph::{Output, SubGraph};
use crate::graph::{Always, Context, FnNode, Graph, GraphBuilder, Map, NodeFuture, Target};
use crate::observe::NoopObserver;
use crate::run::{Outcome, channel, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;

/// The lists the child's first node writes, one per attempt.
struct Script {
    attempt: AtomicUsize,
    lists: Vec<Vec<&'static str>>,
    calls: Mutex<BTreeMap<String, usize>>,
}

impl Script {
    fn new(lists: Vec<Vec<&'static str>>) -> Arc<Self> {
        Arc::new(Self {
            attempt: AtomicUsize::new(0),
            lists,
            calls: Mutex::default(),
        })
    }

    fn first_attempt(&self) -> bool {
        self.attempt.load(Ordering::SeqCst) == 0
    }

    fn list(&self) -> Vec<&'static str> {
        let attempt = self.attempt.load(Ordering::SeqCst);
        self.lists.get(attempt).cloned().unwrap_or_default()
    }

    fn calls(&self, item: &str) -> usize {
        self.calls.lock().unwrap().get(item).copied().unwrap_or(0)
    }
}

/// `list` writes this attempt's items into `inner`, then `fan` maps over them:
/// on the first attempt the item `y` fails, every other item appends its upper
/// case to `out`. Output `out`.
fn child(script: &Arc<Script>) -> Arc<Graph> {
    let schema = Schema::builder()
        .state(key("inner"), Kind::list(Kind::Str))
        .state(key("item"), Kind::Str)
        .state(key("out"), Kind::list(Kind::Str))
        .build();
    let lister = script.clone();
    let list = FnNode::new(
        move |_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            let items: Vec<Value> = lister.list().into_iter().map(Value::str).collect();
            Box::pin(async move {
                Ok(vec![Update::Set {
                    key: key("inner"),
                    value: Value::list(items),
                }])
            })
        },
    );
    let worker = script.clone();
    let body = FnNode::new(
        move |s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            let worker = worker.clone();
            let item = s.str(&key("item")).map(str::to_owned);
            Box::pin(async move {
                let item = item?;
                *worker
                    .calls
                    .lock()
                    .unwrap()
                    .entry(item.clone())
                    .or_default() += 1;
                if worker.first_attempt() && item == "y" {
                    return Err("the item failed".into());
                }
                Ok(vec![Update::Append {
                    key: key("out"),
                    value: Value::str(item.to_uppercase()),
                }])
            })
        },
    );
    let fan = Map {
        list: key("inner"),
        item: key("item"),
        body: Box::new(body),
        max_concurrency: None,
        on_item_failure: crate::graph::ItemFailure::Finish,
    };
    let graph = GraphBuilder::new(schema)
        .entry(nid("list"))
        .node(nid("list"), list)
        .map(nid("fan"), fan)
        .edge(nid("list"), Always(Target::Node(nid("fan"))))
        .edge(nid("fan"), Always(end("done")))
        .output(key("out"))
        .build()
        .unwrap();
    Arc::new(graph)
}

/// Runs the caller once (it fails), then resumes it on the next attempt.
async fn fail_then_resume(script: &Arc<Script>) -> Vec<Value> {
    let schema = Schema::builder()
        .state(key("result"), Kind::list(Kind::Str))
        .build();
    let caller = GraphBuilder::new(schema.clone())
        .entry(nid("call"))
        .subgraph(
            nid("call"),
            SubGraph::call(child(script)).output(key("out"), Output::Set(key("result"))),
        )
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
    assert!(
        checkpoint.pending.is_empty(),
        "nothing below the call is pending"
    );
    script.attempt.store(1, Ordering::SeqCst);
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
    state.list(&key("result")).unwrap().to_vec()
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_restarted_child_whose_items_moved_when_resumed_then_each_item_is_processed() {
    let script = Script::new(vec![vec!["x", "y"], vec!["y", "x"]]);
    let result = fail_then_resume(&script).await;
    assert_eq!(result, vec![Value::str("Y"), Value::str("X")]);
    assert_eq!(script.calls("x"), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_restarted_child_with_the_same_first_item_when_resumed_then_the_call_runs_whole_again()
 {
    let script = Script::new(vec![vec!["x", "y"], vec!["x", "z"]]);
    let result = fail_then_resume(&script).await;
    assert_eq!(result, vec![Value::str("X"), Value::str("Z")]);
    assert_eq!(script.calls("x"), 2);
}
