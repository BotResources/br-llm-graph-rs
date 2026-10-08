//! A map whose body calls a graph: items in, analyses out, in item order.
//! The analysis of one item fails once; the run is resumed from its
//! checkpoint and only that item runs again.

#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use br_llm_graph::{
    Always, Config, Context, EndLabel, FnNode, Graph, GraphBuilder, Input, Kind, Map, NodeFuture,
    NodeId, Outcome, Output, Schema, State, SubGraph, Target, Update, Value, channel, run,
};

use common::{key, quiet_context};

fn id(name: &str) -> NodeId {
    NodeId::new(name).expect("valid node id")
}

fn done() -> Always {
    Always(Target::End(EndLabel::new("done").expect("valid label")))
}

/// Input `item`, output `analysis`. Fails the first time it reads "beta".
fn analyze(calls: Arc<AtomicUsize>, failed_once: Arc<AtomicBool>) -> Arc<Graph> {
    let schema = Schema::builder()
        .state(key("item"), Kind::Str)
        .state(key("analysis"), Kind::Str)
        .build();
    let read = FnNode::new(
        move |s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            calls.fetch_add(1, Ordering::SeqCst);
            let item = s.str(&key("item")).map(str::to_owned);
            let fail = matches!(&item, Ok(text) if text == "beta")
                && !failed_once.swap(true, Ordering::SeqCst);
            Box::pin(async move {
                let item = item?;
                if fail {
                    return Err(format!("could not read {item}").into());
                }
                Ok(vec![Update::Set {
                    key: key("analysis"),
                    value: Value::str(format!("{item}: {} letters", item.len())),
                }])
            })
        },
    );
    let graph = GraphBuilder::new(schema)
        .entry(id("read"))
        .node(id("read"), read)
        .edge(id("read"), done())
        .input(key("item"))
        .output(key("analysis"))
        .build()
        .expect("analysis graph");
    Arc::new(graph)
}

fn each_item(child: Arc<Graph>) -> Graph {
    let schema = Schema::builder()
        .state(key("items"), Kind::list(Kind::Str))
        .state(key("item"), Kind::Str)
        .state(key("analyses"), Kind::list(Kind::Str))
        .build();
    let body = SubGraph::call(child)
        .input(key("item"), Input::From(key("item")))
        .output(key("analysis"), Output::Append(key("analyses")));
    let map = Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(body),
        max_concurrency: None,
    };
    GraphBuilder::new(schema)
        .entry(id("each"))
        .map(id("each"), map)
        .edge(id("each"), done())
        .input(key("items"))
        .build()
        .expect("map graph")
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let calls = Arc::new(AtomicUsize::new(0));
    let graph = each_item(analyze(calls.clone(), Arc::new(AtomicBool::new(false))));
    let items = ["alpha", "beta", "gamma"].map(Value::str).to_vec();
    let state = graph
        .start_state([(key("items"), Value::list(items))])
        .expect("start state");
    let config = Config::new(graph.schema(), BTreeMap::new()).expect("config");
    let ctx = quiet_context();

    let (_sender, mut inbox) = channel();
    let failure = run(&graph, &config, state, None, &ctx, &mut inbox)
        .await
        .err()
        .expect("the first attempt fails");
    println!("first attempt: {}", failure.error);
    for (occurrence, _) in failure.checkpoint.pending.iter() {
        println!("  finished before the failure: {occurrence}");
    }

    let checkpoint = failure.checkpoint;
    let resumed = ctx.with_pending(checkpoint.pending);
    let (_sender, mut inbox) = channel();
    let outcome = run(
        &graph,
        &config,
        checkpoint.state,
        Some(checkpoint.cursor),
        &resumed,
        &mut inbox,
    )
    .await;
    let Ok(Outcome::Finished { state, .. }) = outcome else {
        panic!("the resumed run did not finish");
    };
    for analysis in state.list(&key("analyses")).expect("analyses") {
        println!("  {analysis}");
    }
    println!(
        "analysis calls: {} for 3 items",
        calls.load(Ordering::SeqCst)
    );
}
