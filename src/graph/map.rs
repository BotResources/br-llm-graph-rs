use futures_util::StreamExt;
use futures_util::stream;

use crate::error::GraphError;
use crate::graph::context::Context;
use crate::graph::limit::Limit;
use crate::graph::node::{Node, NodeError, NodeFuture};
use crate::state::{Config, State, Value};
use crate::update::Update;
use crate::value::Key;

/// Repeats `body` over the items of `list` and collects what the bodies
/// append.
///
/// Each body runs on the state with `item` set to its item, in a context for
/// its own occurrence (the map node's segment with the item index). A body may
/// only return `Update::Append` addressed to a list of the graph; the map
/// forwards them, item after item in list order, whatever the order in which
/// the bodies finished. Any other update fails the map (`MapBodyNotAppend`).
///
/// At most `max_concurrency` bodies run at once (every item at once when
/// `None`); a new body starts as soon as any running one finishes. Every body
/// runs to its end; the first error in item order fails the map.
pub struct Map {
    pub list: Key,
    pub item: Key,
    pub body: Box<dyn Node>,
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
            let mut runs = Vec::with_capacity(items.len());
            for (index, item) in items.into_iter().enumerate() {
                let derived = state.derive(&self.item, item)?;
                let item_ctx = item_context(ctx, index)?;
                runs.push(self.run_item(index, derived, config, item_ctx));
            }
            let mut results: Vec<(usize, Result<Vec<Update>, NodeError>)> =
                stream::iter(runs).buffer_unordered(width).collect().await;
            results.sort_by_key(|(index, _)| *index);
            let mut forwarded = Vec::new();
            for (_, result) in results {
                forwarded.extend(result?);
            }
            Ok(forwarded)
        })
    }
}

impl Map {
    async fn run_item(
        &self,
        index: usize,
        derived: State,
        config: &Config,
        ctx: Context,
    ) -> (usize, Result<Vec<Update>, NodeError>) {
        let result = match self.body.run(&derived, config, &ctx).await {
            Ok(updates) => check_appends(&derived, &updates)
                .map(|()| updates)
                .map_err(NodeError::from),
            Err(error) => Err(error),
        };
        (index, result)
    }
}

/// The context of item `index`: the map node's occurrence with the index.
fn item_context(ctx: &Context, index: usize) -> Result<Context, GraphError> {
    let occurrence = ctx
        .occurrence()
        .item(index)
        .ok_or(GraphError::MapWithoutOccurrence)?;
    Ok(ctx.with_occurrence(occurrence))
}

fn check_appends(state: &State, updates: &[Update]) -> Result<(), GraphError> {
    for update in updates {
        match update {
            Update::Append { key, value } => state.check_append(key, value)?,
            Update::Set { key, .. }
            | Update::Input { key, .. }
            | Update::PushTurn { key, .. }
            | Update::PushStep { key, .. }
            | Update::PushResult { key, .. } => {
                return Err(GraphError::MapBodyNotAppend { key: key.clone() });
            }
        }
    }
    Ok(())
}
