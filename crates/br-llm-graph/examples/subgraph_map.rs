//! A map whose body calls a graph: items in, analyses out, in item order.
//! The analysis of one item fails once; the run is resumed from its
//! checkpoint and only that item runs again.

#[allow(dead_code)]
mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use br_llm_graph::{
    Always, Config, Context, EndLabel, FnNode, Graph, GraphBuilder, ItemFailure, Map, NodeFuture,
    NodeId, Outcome, Schema, State, StateKey, SubGraph, Target, control, run,
};

use common::quiet_context;

fn id(name: &str) -> NodeId {
    NodeId::new(name).expect("valid node id")
}

fn done() -> Always {
    Always(Target::End(EndLabel::new("done").expect("valid label")))
}

/// The child's keys: input `item`, output `analysis`.
struct Child {
    item: StateKey<String>,
    analysis: StateKey<String>,
}

/// The caller's keys: the list, its item, the analyses.
struct Caller {
    items: StateKey<Vec<String>>,
    item: StateKey<String>,
    analyses: StateKey<Vec<String>>,
}

/// The analysis of one item, and its keys. Fails the first time it reads
/// "beta".
fn analyze(calls: Arc<AtomicUsize>, failed_once: Arc<AtomicBool>) -> (Arc<Graph>, Child) {
    let mut schema = Schema::builder();
    let child = Child {
        item: schema.state("item").expect("key"),
        analysis: schema.state("analysis").expect("key"),
    };
    let (read, write) = (child.item.clone(), child.analysis.clone());
    let node = FnNode::new(
        move |s: &State, _: &Config, _: &Context| -> NodeFuture<'_> {
            calls.fetch_add(1, Ordering::SeqCst);
            let item = s.get(&read).map(str::to_owned);
            let fail = matches!(&item, Ok(text) if text == "beta")
                && !failed_once.swap(true, Ordering::SeqCst);
            let write = write.clone();
            Box::pin(async move {
                let item = item?;
                if fail {
                    return Err(format!("could not read {item}").into());
                }
                Ok(vec![write.set(format!("{item}: {} letters", item.len()))?])
            })
        },
    );
    let graph = GraphBuilder::new(schema.build())
        .entry(id("read"))
        .node(id("read"), node)
        .edge(id("read"), done())
        .input(&child.item)
        .output(&child.analysis)
        .build()
        .expect("analysis graph");
    (Arc::new(graph), child)
}

/// The map: one call of `child` per item, its analysis appended.
fn each_item(caller: &Caller, schema: Schema, keys: &Child, child: Arc<Graph>) -> Graph {
    let body = SubGraph::call(child)
        .input_from(&keys.item, &caller.item)
        .output_append(&keys.analysis, &caller.analyses);
    let map = Map::new(&caller.items, &caller.item, body).on_item_failure(ItemFailure::Finish);
    GraphBuilder::new(schema)
        .entry(id("each"))
        .map(id("each"), map)
        .edge(id("each"), done())
        .input(&caller.items)
        .build()
        .expect("map graph")
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut schema = Schema::builder();
    let caller = Caller {
        items: schema.state("items").expect("key"),
        item: schema.state("item").expect("key"),
        analyses: schema.state("analyses").expect("key"),
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let (analysis, child) = analyze(calls.clone(), Arc::new(AtomicBool::new(false)));
    let graph = each_item(&caller, schema.build(), &child, analysis);
    let items = ["alpha", "beta", "gamma"].map(str::to_owned).to_vec();
    let state = graph
        .start_state([caller.items.given(items).expect("items")])
        .expect("start state");
    let config = Config::new(graph.schema(), Default::default()).expect("config");
    let ctx = quiet_context();

    let (_sender, mut channel) = control();
    let failure = run(&graph, &config, state, None, &ctx, &mut channel)
        .await
        .err()
        .expect("the first attempt fails");
    println!("first attempt: {}", failure.error);
    for (occurrence, _) in failure.checkpoint.pending.iter() {
        println!("  finished before the failure: {occurrence}");
    }

    let checkpoint = failure.checkpoint;
    let resumed = ctx.with_pending(checkpoint.pending);
    let (_sender, mut channel) = control();
    let outcome = run(
        &graph,
        &config,
        checkpoint.state,
        Some(checkpoint.cursor),
        &resumed,
        &mut channel,
    )
    .await;
    let Ok(Outcome::Finished { state, .. }) = outcome else {
        panic!("the resumed run did not finish");
    };
    for analysis in state.get(&caller.analyses).expect("analyses") {
        println!("  {analysis}");
    }
    println!(
        "analysis calls: {} for 3 items",
        calls.load(Ordering::SeqCst)
    );
}
