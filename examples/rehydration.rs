#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_llm_graph::{
    Always, Checkpoint, Config, Context, EndLabel, FnNode, Graph, GraphBuilder, Kind, NodeFuture,
    NodeId, Outcome, Schema, Session, State, Target, Update, Value, channel, run,
};

use common::{key, quiet_context};

fn set_count(value: i64) -> impl br_llm_graph::Node {
    FnNode::new(
        move |_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            Box::pin(async move {
                Ok(vec![Update::Set {
                    key: key("count"),
                    value: Value::int(value),
                }])
            })
        },
    )
}

fn schema() -> Schema {
    Schema::builder().state(key("count"), Kind::Int).build()
}

fn state() -> State {
    let mut values = BTreeMap::new();
    values.insert(key("count"), Value::int(0));
    State::new(schema(), values).expect("state")
}

fn linear_graph() -> Arc<Graph> {
    Arc::new(
        GraphBuilder::new(schema())
            .entry(NodeId::new("a").expect("id"))
            .node(NodeId::new("a").expect("id"), set_count(5))
            .node(NodeId::new("b").expect("id"), set_count(6))
            .edge(
                NodeId::new("a").expect("id"),
                Always(Target::Node(NodeId::new("b").expect("id"))),
            )
            .edge(
                NodeId::new("b").expect("id"),
                Always(Target::End(EndLabel::new("done").expect("label"))),
            )
            .build()
            .expect("build"),
    )
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    pause_then_resume().await;
    cancel_then_resume().await;
}

async fn pause_then_resume() {
    let graph = linear_graph();
    let mut session = Session::new(
        graph.clone(),
        Config::new(&schema(), BTreeMap::new()).expect("config"),
        state(),
        quiet_context(),
    );
    session.sender().pause();
    let outcome = session.run_once().await.expect("run");
    let Outcome::Paused { checkpoint } = outcome else {
        println!("rehydration: expected a pause");
        return;
    };
    let json = serde_json::to_string(&checkpoint).expect("json");
    println!(
        "rehydration: paused, checkpoint serialized ({} bytes)",
        json.len()
    );
    drop(session);

    let restored: Checkpoint = serde_json::from_str(&json).expect("restore");
    let mut resumed = Session::resume(
        graph,
        Config::new(&schema(), BTreeMap::new()).expect("config"),
        restored,
        quiet_context(),
    )
    .expect("resume");
    let outcome = resumed.run_once().await.expect("run");
    match outcome {
        Outcome::Finished { state, end } => println!(
            "rehydration: resumed to End({end}) with count={}",
            state.int(&key("count")).expect("int")
        ),
        _ => println!("rehydration: unexpected outcome"),
    }
}

async fn cancel_then_resume() {
    let flag = Arc::new(AtomicUsize::new(0));
    let node_flag = flag.clone();
    let graph = GraphBuilder::new(schema())
        .entry(NodeId::new("work").expect("id"))
        .node(
            NodeId::new("work").expect("id"),
            FnNode::new(
                move |_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
                    let flag = node_flag.clone();
                    Box::pin(async move {
                        if flag.fetch_add(1, Ordering::SeqCst) == 0 {
                            futures_util::future::pending::<()>().await;
                        }
                        Ok(vec![Update::Set {
                            key: key("count"),
                            value: Value::int(9),
                        }])
                    })
                },
            ),
        )
        .edge(
            NodeId::new("work").expect("id"),
            Always(Target::End(EndLabel::new("done").expect("label"))),
        )
        .build()
        .expect("build");

    let (sender, mut inbox) = channel();
    sender.cancel();
    let cancelled = run(
        &graph,
        &Config::new(&schema(), BTreeMap::new()).expect("config"),
        state(),
        None,
        &quiet_context(),
        &mut inbox,
    )
    .await
    .expect("run");
    let Outcome::Cancelled { checkpoint } = cancelled else {
        println!("rehydration: expected a cancel");
        return;
    };
    println!(
        "rehydration: cancelled mid-superstep, count still {}",
        checkpoint.state.int(&key("count")).expect("int")
    );

    let (_sender, mut inbox) = channel();
    let resumed = run(
        &graph,
        &Config::new(&schema(), BTreeMap::new()).expect("config"),
        checkpoint.state,
        Some(checkpoint.cursor),
        &quiet_context(),
        &mut inbox,
    )
    .await
    .expect("run");
    match resumed {
        Outcome::Finished { state, end } => println!(
            "rehydration: cancelled superstep re-ran, End({end}) with count={}",
            state.int(&key("count")).expect("int")
        ),
        _ => println!("rehydration: unexpected outcome"),
    }
}
