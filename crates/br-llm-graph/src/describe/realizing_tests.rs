//! A node carrying realizes labels runs and checks exactly as its node does:
//! the library never reads the labels.

use std::sync::Arc;

use super::test_support::*;
use crate::error::GraphError;
use crate::graph::{Always, CheckSite, Context, FnNode, GraphBuilder, Map, Node, NodeFuture};
use crate::observe::NoopObserver;
use crate::run::{Outcome, control, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;

fn schema() -> Schema {
    Schema::builder()
        .state_by_name(key("count"), Kind::Int)
        .state_by_name(key("items"), Kind::list(Kind::Str))
        .state_by_name(key("item"), Kind::Str)
        .state_by_name(key("outs"), Kind::list(Kind::Str))
        .build()
}

fn ctx() -> Context {
    Context::new(
        Arc::new(NoopObserver),
        Arc::new(crate::testkit::SeqIds::new()),
    )
}

fn counting() -> impl Node {
    FnNode::new(|_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
        Box::pin(async {
            Ok(vec![Update::Set {
                key: key("count"),
                value: Value::int(7),
            }])
        })
    })
}

/// A node whose check refuses, and that says so through `Realizing`.
struct Refusing;

impl Node for Refusing {
    fn run<'a>(&'a self, _s: &'a State, _c: &'a Config, _x: &'a Context) -> NodeFuture<'a> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn check(&self, _schema: &Schema, site: CheckSite) -> Result<(), GraphError> {
        match site {
            CheckSite::MapBody => Err(GraphError::MapBodySet { key: key("count") }),
            _ => Ok(()),
        }
    }
}

#[tokio::test]
async fn given_a_realizing_node_when_run_then_it_gives_its_nodes_updates() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), counting().realizing(["Count"]))
        .edge(nid("a"), Always(end("done")))
        .build()
        .unwrap();
    let state = graph.start_state([]).unwrap();
    let config = Config::new(&schema(), Default::default()).unwrap();
    let (_sender, mut channel) = control();
    let Ok(Outcome::Finished { state, .. }) =
        run(&graph, &config, state, None, &ctx(), &mut channel).await
    else {
        panic!("the run finishes");
    };
    assert_eq!(state.int(&key("count")).unwrap(), 7);
}

#[test]
fn given_a_realizing_node_when_checked_then_it_forwards_the_check_site_included() {
    let map = Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(Refusing.realizing(["Body"])),
        max_concurrency: None,
        on_item_failure: crate::graph::ItemFailure::Finish,
    };
    let result = GraphBuilder::new(schema())
        .entry(nid("m"))
        .node(nid("m"), map.realizing(["Map"]))
        .edge(nid("m"), Always(end("done")))
        .build();
    assert!(matches!(
        crate::testkit::refusal(result),
        GraphError::MapBodySet { .. }
    ));
}
