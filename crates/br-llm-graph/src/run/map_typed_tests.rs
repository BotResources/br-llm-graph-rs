//! A map built from typed keys, its limit and a count read from typed
//! configuration keys.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::GraphError;
use crate::graph::{
    Always, CaptureUpdate, Context, Count, GraphBuilder, ItemFailure, Limit, Map, Node, NodeFuture,
    Target,
};
use crate::observe::NoopObserver;
use crate::run::control::control;
use crate::run::outcome::Outcome;
use crate::run::runner::run;
use crate::state::{Config, ConfigKey, Schema, State, StateKey, Value};
use crate::value::{EndLabel, NodeId};

/// Appends `<item>!` to `outs`; fails on the item `bad`.
struct Shout {
    item: StateKey<String>,
    outs: StateKey<Vec<String>>,
}

impl Node for Shout {
    fn run<'a>(&'a self, state: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let item = state.get(&self.item)?;
            if item == "bad" {
                return Err("cannot shout bad".into());
            }
            Ok(vec![self.outs.append(format!("{item}!"))?])
        })
    }
}

#[tokio::test]
async fn given_a_map_of_typed_keys_when_run_then_each_item_through_the_body_and_errors_captured() {
    let mut schema = Schema::builder();
    let items: StateKey<Vec<String>> = schema.state("items").unwrap();
    let item: StateKey<String> = schema.state("item").unwrap();
    let outs: StateKey<Vec<String>> = schema.state("outs").unwrap();
    let errors: StateKey<Vec<String>> = schema.state("errors").unwrap();
    let width: ConfigKey<i64> = schema.config("width").unwrap();
    let schema = schema.build();
    let body = Shout {
        item: item.clone(),
        outs: outs.clone(),
    };
    let map = Map::new(&items, &item, body)
        .max_concurrency(Limit::config(&width))
        .on_item_failure(ItemFailure::Capture(vec![CaptureUpdate::append_reason(
            &errors,
        )]));
    let m = NodeId::new("m").unwrap();
    let graph = GraphBuilder::new(schema.clone())
        .entry(m.clone())
        .map(m.clone(), map)
        .edge(m, Always(Target::End(EndLabel::new("done").unwrap())))
        .input(&items)
        .build()
        .unwrap();
    let state = graph
        .start_state([items
            .given(vec!["a".to_owned(), "bad".to_owned(), "c".to_owned()])
            .unwrap()])
        .unwrap();
    let config = Config::new(
        &schema,
        BTreeMap::from([(width.key().clone(), Value::int(1))]),
    )
    .unwrap();
    let ctx = crate::testkit::context(Arc::new(NoopObserver));
    let (_sender, mut channel) = control();
    let Ok(Outcome::Finished { state, .. }) =
        run(&graph, &config, state, None, &ctx, &mut channel).await
    else {
        panic!("the run did not finish");
    };
    assert_eq!(state.get(&outs).unwrap(), vec!["a!", "c!"]);
    let errors = state.get(&errors).unwrap();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("cannot shout bad"), "{errors:?}");
}

#[test]
fn given_a_map_of_typed_keys_when_built_then_every_item_at_once_and_finish_unless_set() {
    let mut schema = Schema::builder();
    let items: StateKey<Vec<i64>> = schema.state("items").unwrap();
    let item: StateKey<i64> = schema.state("item").unwrap();
    let width: ConfigKey<i64> = schema.config("width").unwrap();
    let body = || crate::graph::FnNode::new(|_, _, _| Box::pin(async { Ok(Vec::new()) }));
    let map = Map::new(&items, &item, body());
    assert_eq!(map.list.as_str(), "items");
    assert_eq!(map.item.as_str(), "item");
    assert_eq!(map.max_concurrency, None);
    assert_eq!(map.on_item_failure, ItemFailure::Finish);
    let map = Map::new(&items, &item, body())
        .max_concurrency(Limit::config(&width))
        .on_item_failure(ItemFailure::FailFast);
    assert_eq!(
        map.max_concurrency,
        Some(Limit::Config(width.key().clone()))
    );
    assert_eq!(map.on_item_failure, ItemFailure::FailFast);
}

#[test]
fn given_a_limit_and_a_count_on_typed_configuration_keys_when_checked_and_resolved_then_read() {
    let mut schema = Schema::builder();
    let width: ConfigKey<i64> = schema.config("width").unwrap();
    let budget: ConfigKey<i64> = schema.config("budget").unwrap();
    let schema = schema.build();
    let config = Config::new(
        &schema,
        BTreeMap::from([
            (width.key().clone(), Value::int(2)),
            (budget.key().clone(), Value::int(0)),
        ]),
    )
    .unwrap();
    assert_eq!(Limit::config(&width), Limit::Config(width.key().clone()));
    assert!(Limit::config(&width).check(&schema).is_ok());
    assert_eq!(Limit::config(&width).resolve(&config).unwrap().get(), 2);
    assert!(Count::config(&budget).check(&schema).is_ok());
    assert_eq!(Count::config(&budget).resolve(&config).unwrap(), 0);
    let other = Schema::builder().build();
    assert!(matches!(
        Count::config(&budget).check(&other),
        Err(GraphError::CountKeyMismatch { .. })
    ));
}
