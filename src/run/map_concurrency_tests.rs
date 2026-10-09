use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::error::{GraphError, NodeFault};
use crate::graph::{Context, Graph, GraphBuilder, Limit, Map, Node, NodeFuture};
use crate::observe::NoopObserver;
use crate::run::inbox::channel;
use crate::run::outcome::{Outcome, RunFailure};
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;

fn ctx() -> Context {
    Context::new(
        Arc::new(NoopObserver),
        Arc::new(crate::testkit::SeqIds::new()),
    )
}

fn width_schema() -> Schema {
    let mut schema = schema();
    schema.config.insert(key("width"), Kind::Int);
    schema
}

fn width_config(width: i64) -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("width"), Value::int(width));
    Config::new(&width_schema(), values).unwrap()
}

#[derive(Default)]
struct Gauge {
    live: AtomicUsize,
    peak: AtomicUsize,
}

struct GaugedBody(Arc<Gauge>);

impl Node for GaugedBody {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let now = self.0.live.fetch_add(1, Ordering::SeqCst) + 1;
            self.0.peak.fetch_max(now, Ordering::SeqCst);
            for _ in 0..3 {
                tokio::task::yield_now().await;
            }
            self.0.live.fetch_sub(1, Ordering::SeqCst);
            let item = state.str(&key("item"))?.to_owned();
            Ok(vec![Update::Append {
                key: key("outs"),
                value: Value::str(format!("done {item}")),
            }])
        })
    }
}

fn map_graph(gauge: &Arc<Gauge>, max_concurrency: Option<Limit>) -> Result<Graph, GraphError> {
    let map = Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(GaugedBody(gauge.clone())),
        max_concurrency,
        on_item_failure: crate::graph::ItemFailure::Finish,
    };
    GraphBuilder::new(width_schema())
        .entry(nid("m"))
        .map(nid("m"), map)
        .edge(nid("m"), edge(vec![end("done")]))
        .build()
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

async fn run_map(limit: Option<Limit>, config: Config) -> (Result<Outcome, RunFailure>, usize) {
    let gauge = Arc::new(Gauge::default());
    let graph = map_graph(&gauge, limit).unwrap();
    let (_sender, mut inbox) = channel();
    let items = ["a", "b", "c", "d", "e"];
    let result = run(&graph, &config, seeded(&items), None, &ctx(), &mut inbox).await;
    (result, gauge.peak.load(Ordering::SeqCst))
}

fn outs(result: Result<Outcome, RunFailure>) -> Vec<Value> {
    match result.map_err(|f| f.error).unwrap() {
        Outcome::Finished { state, .. } => state.list(&key("outs")).unwrap().to_vec(),
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

#[tokio::test]
async fn given_fixed_limit_when_map_runs_then_at_most_that_many_bodies_at_once_in_item_order() {
    let limit = Limit::Fixed(NonZeroUsize::new(2).unwrap());
    let (result, peak) = run_map(Some(limit), width_config(9)).await;
    assert_eq!(peak, 2);
    let expected: Vec<Value> = ["a", "b", "c", "d", "e"]
        .iter()
        .map(|item| Value::str(format!("done {item}")))
        .collect();
    assert_eq!(outs(result), expected);
}

#[tokio::test]
async fn given_no_limit_when_map_runs_then_every_body_runs_at_once() {
    let (result, peak) = run_map(None, width_config(9)).await;
    assert_eq!(peak, 5);
    assert_eq!(outs(result).len(), 5);
}

#[tokio::test]
async fn given_config_limit_when_map_runs_then_the_config_value_bounds_it() {
    let (result, peak) = run_map(Some(Limit::Config(key("width"))), width_config(3)).await;
    assert_eq!(peak, 3);
    assert_eq!(outs(result).len(), 5);
}

#[tokio::test]
async fn given_config_limit_below_one_when_map_runs_then_node_fails_with_typed_limit_error() {
    let (result, peak) = run_map(Some(Limit::Config(key("width"))), width_config(0)).await;
    assert_eq!(peak, 0);
    let error = result.err().unwrap().error;
    let GraphError::NodeFailed {
        node,
        source: NodeFault::Returned(returned),
    } = error
    else {
        panic!("expected a node failure");
    };
    assert_eq!(node, nid("m"));
    assert!(matches!(
        returned.downcast_ref::<GraphError>(),
        Some(GraphError::LimitNotPositive { value: 0, .. })
    ));
}

#[test]
fn given_limit_on_a_key_that_is_not_an_int_config_key_when_built_then_refused() {
    let gauge = Arc::new(Gauge::default());
    for name in ["count", "missing"] {
        assert!(matches!(
            crate::testkit::refusal(map_graph(&gauge, Some(Limit::Config(key(name))))),
            GraphError::LimitKeyMismatch { .. }
        ));
    }
}
