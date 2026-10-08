use futures_util::StreamExt;
use futures_util::stream;

use crate::graph::context::Context;
use crate::graph::limit::Limit;
use crate::graph::node::{Node, NodeFuture};
use crate::state::{Config, State, Value};
use crate::update::Update;
use crate::value::Key;

pub struct Map {
    pub list: Key,
    pub item: Key,
    pub body: Box<dyn Node>,
    pub output: Key,
    pub results: Key,
    pub max_concurrency: Option<Limit>,
}

impl Node for Map {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let items: Vec<Value> = state.list(&self.list)?.to_vec();
            let width = match &self.max_concurrency {
                Some(limit) => limit.resolve(config)?.get(),
                None => items.len().max(1),
            };
            let mut deriveds = Vec::with_capacity(items.len());
            for item in items {
                deriveds.push(state.derive(&self.item, item)?);
            }
            let runs: Vec<NodeFuture<'_>> = deriveds
                .iter()
                .map(|derived| self.body.run(derived, config, ctx))
                .collect();
            let outputs: Vec<_> = stream::iter(runs).buffered(width).collect().await;
            let mut appends = Vec::with_capacity(deriveds.len());
            for (derived, output) in deriveds.iter_mut().zip(outputs) {
                let updates = output?;
                derived.apply_batch(&updates)?;
                for update in &updates {
                    ctx.observer.applied(ctx.origin(), update);
                }
                let value = derived.get(&self.output)?.clone();
                appends.push(Update::Append {
                    key: self.results.clone(),
                    value,
                });
            }
            Ok(appends)
        })
    }
}
