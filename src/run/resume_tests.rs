use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use futures_util::future::join;

use crate::graph::{Context, Graph, GraphBuilder, Map, Node};
use crate::observe::{NoopObserver, Observer};
use crate::origin::Origin;
use crate::run::counted::{Counted, CountedBody};
use crate::run::gates::{GatedBody, Gates};
use crate::run::inbox::channel;
use crate::run::outcome::{Outcome, RunFailure};
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::run::{Checkpoint, PendingWrites};
use crate::state::{State, Value};
use crate::update::Update;

fn ctx(observer: Arc<dyn Observer>) -> Context {
    crate::testkit::context(observer)
}

fn map_then_next(body: impl Node + 'static) -> Graph {
    let map = Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(body),
        max_concurrency: None,
    };
    GraphBuilder::new(schema())
        .entry(nid("m"))
        .map(nid("m"), map)
        .node(nid("next"), noop_node())
        .edge(nid("m"), edge(vec![to("next")]))
        .edge(nid("next"), edge(vec![end("done")]))
        .build()
        .unwrap()
}

fn counted(counted: &Arc<Counted>) -> CountedBody {
    CountedBody {
        counted: counted.clone(),
        item: key("item"),
        outs: key("outs"),
        log: key("log"),
    }
}

fn seeded(items: &[&str]) -> State {
    let mut state = base_state();
    state
        .apply_batch(&[Update::Set {
            key: key("items"),
            value: Value::list(items.iter().map(|s| Value::str(*s)).collect()),
        }])
        .unwrap();
    state
}

fn keys(pending: &PendingWrites) -> Vec<String> {
    pending.iter().map(|(key, _)| key.to_string()).collect()
}

fn finished_state(result: Result<Outcome, RunFailure>) -> State {
    match result {
        Ok(Outcome::Finished { state, .. }) => state,
        Ok(_) => panic!("expected finished"),
        Err(failure) => panic!("expected finished, got {}", failure.error),
    }
}

async fn resume(graph: &Graph, checkpoint: Checkpoint, observer: Arc<dyn Observer>) -> State {
    let (_sender, mut inbox) = channel();
    let ctx = ctx(observer).with_pending(checkpoint.pending);
    let result = run(
        graph,
        &config(),
        checkpoint.state,
        Some(checkpoint.cursor),
        &ctx,
        &mut inbox,
    )
    .await;
    finished_state(result)
}

async fn uninterrupted(items: &[&str]) -> State {
    let graph = map_then_next(counted(&Arc::new(Counted::default())));
    let (_sender, mut inbox) = channel();
    let ctx = ctx(Arc::new(NoopObserver));
    finished_state(run(&graph, &config(), seeded(items), None, &ctx, &mut inbox).await)
}

const ITEMS: [&str; 5] = ["a", "b", "c", "d", "e"];

#[tokio::test]
async fn given_an_item_failing_after_others_finished_when_resumed_then_only_unfinished_items_run() {
    let calls = Counted::failing_on("c");
    let graph = map_then_next(counted(&calls));
    let (_sender, mut inbox) = channel();
    let ctx = ctx(Arc::new(NoopObserver));
    let failure = run(&graph, &config(), seeded(&ITEMS), None, &ctx, &mut inbox)
        .await
        .err()
        .unwrap();
    let checkpoint = failure.checkpoint;
    assert_eq!(
        keys(&checkpoint.pending),
        vec!["m[0]", "m[1]", "m[3]", "m[4]"]
    );
    assert!(checkpoint.state.list(&key("outs")).unwrap().is_empty());
    assert_eq!(checkpoint.cursor.active, vec![nid("m")]);

    calls.heal();
    let json = serde_json::to_string(&checkpoint).unwrap();
    let restored: Checkpoint = serde_json::from_str(&json).unwrap();
    let state = resume(&graph, restored, Arc::new(NoopObserver)).await;
    let expected: BTreeMap<String, usize> = [("a", 1), ("b", 1), ("c", 2), ("d", 1), ("e", 1)]
        .into_iter()
        .map(|(item, count)| (item.to_owned(), count))
        .collect();
    assert_eq!(calls.calls(), expected);
    assert_eq!(state, uninterrupted(&ITEMS).await);
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_cancel_in_the_middle_of_a_map_when_resumed_then_only_unfinished_items_run_again() {
    let items = ["a", "b", "c", "d"];
    let gates = Gates::new(&items);
    let graph = map_then_next(GatedBody {
        gates: gates.clone(),
        item: key("item"),
        lists: vec![key("outs"), key("log")],
    });
    let (sender, mut inbox) = channel();
    let (ctx, config) = (ctx(Arc::new(NoopObserver)), config());
    let running = run(&graph, &config, seeded(&items), None, &ctx, &mut inbox);
    let driver = async {
        gates.release("b").await;
        gates.release("d").await;
        sender.cancel();
    };
    let (result, ()) = join(running, driver).await;
    let Ok(Outcome::Cancelled { checkpoint }) = result else {
        panic!("expected cancelled");
    };
    assert_eq!(keys(&checkpoint.pending), vec!["m[1]", "m[3]"]);
    assert!(checkpoint.state.list(&key("outs")).unwrap().is_empty());

    let state = resume(&graph, checkpoint, Arc::new(NoopObserver)).await;
    assert_eq!(gates.started(), vec!["a", "b", "c", "d", "a", "c"]);
    assert_eq!(state.list(&key("outs")).unwrap(), &items.map(Value::str));
    assert_eq!(
        state.list(&key("log")).unwrap(),
        &["a@log", "b@log", "c@log", "d@log"].map(Value::str)
    );
}

#[derive(Default)]
struct Records(Mutex<Vec<(String, usize)>>);

impl Observer for Records {
    fn recorded(&self, origin: &Origin, updates: &[Update]) {
        let entry = (origin.occurrence.to_string(), updates.len());
        self.0.lock().unwrap().push(entry);
    }
}

#[tokio::test]
async fn given_a_map_superstep_that_completed_when_paused_then_items_were_recorded_and_dropped() {
    let records = Arc::new(Records::default());
    let graph = map_then_next(counted(&Arc::new(Counted::default())));
    let (sender, mut inbox) = channel();
    sender.pause();
    let ctx = ctx(records.clone());
    let outcome = run(
        &graph,
        &config(),
        seeded(&["a", "b", "c"]),
        None,
        &ctx,
        &mut inbox,
    )
    .await;
    let Ok(Outcome::Paused { checkpoint }) = outcome else {
        panic!("expected paused");
    };
    assert!(checkpoint.pending.is_empty());
    assert_eq!(checkpoint.cursor.active, vec![nid("next")]);
    let mut seen = records.0.lock().unwrap().clone();
    seen.sort();
    assert_eq!(
        seen,
        vec![
            ("m[0]".to_owned(), 2),
            ("m[1]".to_owned(), 2),
            ("m[2]".to_owned(), 2)
        ]
    );
}

#[tokio::test]
async fn given_two_runs_sharing_one_context_when_one_fails_then_the_other_does_not_see_its_records()
{
    let failing = Counted::failing_on("b");
    let ctx = ctx(Arc::new(NoopObserver));
    let (_sender, mut inbox) = channel();
    let graph = map_then_next(counted(&failing));
    let failure = run(
        &graph,
        &config(),
        seeded(&["a", "b"]),
        None,
        &ctx,
        &mut inbox,
    )
    .await
    .err()
    .unwrap();
    assert_eq!(keys(&failure.checkpoint.pending), vec!["m[0]"]);
    assert!(ctx.pending().is_empty());

    let fresh = Arc::new(Counted::default());
    let graph = map_then_next(counted(&fresh));
    let (_sender, mut inbox) = channel();
    let state = finished_state(
        run(
            &graph,
            &config(),
            seeded(&["x", "y"]),
            None,
            &ctx,
            &mut inbox,
        )
        .await,
    );
    assert_eq!(
        state.list(&key("outs")).unwrap(),
        &["X", "Y"].map(Value::str)
    );
}
