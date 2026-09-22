use futures_util::future::join_all;

use crate::graph::context::Context;
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
}

impl Node for Map {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let items: Vec<Value> = state.list(&self.list)?.to_vec();
            let mut deriveds = Vec::with_capacity(items.len());
            for item in items {
                deriveds.push(state.derive(&self.item, item)?);
            }
            let futures: Vec<_> = deriveds
                .iter()
                .map(|derived| self.body.run(derived, config, ctx))
                .collect();
            let outputs = join_all(futures).await;
            let mut appends = Vec::with_capacity(deriveds.len());
            for (derived, output) in deriveds.iter_mut().zip(outputs) {
                let updates = output?;
                derived.apply_batch(&updates)?;
                for update in &updates {
                    ctx.observer.applied(update);
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
