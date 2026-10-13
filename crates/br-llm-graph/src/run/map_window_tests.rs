use std::num::NonZeroUsize;
use std::sync::Arc;

use futures_util::future::join;

use crate::graph::{Context, Graph, GraphBuilder, Limit, Map};
use crate::observe::NoopObserver;
use crate::run::gates::{GatedBody, Gates};
use crate::run::inbox::channel;
use crate::run::outcome::{Outcome, RunFailure};
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::{State, Value};
use crate::update::Update;

fn ctx() -> Context {
    Context::new(
        Arc::new(NoopObserver),
        Arc::new(crate::testkit::SeqIds::new()),
    )
}

fn gated_graph(gates: &Arc<Gates>, lists: &[&str], width: Option<usize>) -> Graph {
    let map = Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(GatedBody {
            gates: gates.clone(),
            item: key("item"),
            lists: lists.iter().map(|name| key(name)).collect(),
        }),
        max_concurrency: width.map(|w| Limit::Fixed(NonZeroUsize::new(w).unwrap())),
        on_item_failure: crate::graph::ItemFailure::Finish,
    };
    GraphBuilder::new(schema())
        .entry(nid("m"))
        .map(nid("m"), map)
        .edge(nid("m"), edge(vec![end("done")]))
        .build()
        .unwrap()
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

fn texts(result: &Result<Outcome, RunFailure>, list: &str) -> Vec<String> {
    let Ok(Outcome::Finished { state, .. }) = result else {
        panic!("expected finished");
    };
    state
        .list(&key(list))
        .unwrap()
        .iter()
        .map(|value| match value {
            Value::Str(text) => text.clone(),
            _ => String::new(),
        })
        .collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[tokio::test(flavor = "current_thread")]
async fn given_bodies_finishing_in_reverse_order_when_mapped_then_results_follow_item_order() {
    let items = ["a", "b", "c", "d"];
    let gates = Gates::new(&items);
    let graph = gated_graph(&gates, &["outs"], None);
    let (_sender, mut inbox) = channel();
    let (ctx, config) = (ctx(), config());
    let running = run(&graph, &config, seeded(&items), None, &ctx, &mut inbox);
    let driver = async {
        gates.until(|g| g.started().len() == 4).await;
        for item in ["d", "c", "b", "a"] {
            gates.release(item).await;
        }
    };
    let (result, ()) = join(running, driver).await;
    assert_eq!(gates.finished(), strings(&["d", "c", "b", "a"]));
    assert_eq!(texts(&result, "outs"), strings(&items));
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_slow_first_item_when_mapped_with_width_two_then_later_items_start_and_finish_first()
 {
    let items = ["a", "b", "c", "d", "e"];
    let gates = Gates::new(&items);
    let graph = gated_graph(&gates, &["outs"], Some(2));
    let (_sender, mut inbox) = channel();
    let (ctx, config) = (ctx(), config());
    let running = run(&graph, &config, seeded(&items), None, &ctx, &mut inbox);
    let driver = async {
        gates.until(|g| g.started().len() == 2).await;
        assert_eq!(gates.started(), strings(&["a", "b"]));
        for item in ["b", "c", "d", "e"] {
            gates.release(item).await;
            assert!(gates.started().len() <= gates.finished().len() + 2);
        }
        gates.release("a").await;
    };
    let (result, ()) = join(running, driver).await;
    assert_eq!(gates.started(), strings(&items));
    assert_eq!(gates.finished(), strings(&["b", "c", "d", "e", "a"]));
    assert_eq!(texts(&result, "outs"), strings(&items));
}

#[tokio::test(flavor = "current_thread")]
async fn given_bodies_appending_to_two_lists_out_of_order_when_mapped_then_both_lists_stay_aligned()
{
    let items = ["a", "b", "c"];
    let gates = Gates::new(&items);
    let graph = gated_graph(&gates, &["outs", "log"], Some(3));
    let (_sender, mut inbox) = channel();
    let (ctx, config) = (ctx(), config());
    let running = run(&graph, &config, seeded(&items), None, &ctx, &mut inbox);
    let driver = async {
        for item in ["b", "c", "a"] {
            gates.release(item).await;
        }
    };
    let (result, ()) = join(running, driver).await;
    assert_eq!(texts(&result, "outs"), strings(&items));
    assert_eq!(texts(&result, "log"), strings(&["a@log", "b@log", "c@log"]));
}
