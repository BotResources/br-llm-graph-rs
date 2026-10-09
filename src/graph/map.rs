use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::StreamExt;
use futures_util::stream;

use crate::error::GraphError;
use crate::graph::context::Context;
use crate::graph::limit::Limit;
use crate::graph::node::{Node, NodeError, NodeFuture};
use crate::graph::subgraph::{CaptureUpdate, captured, check_capture};
use crate::state::{Config, Kind, Schema, State, Value};
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
/// `None`); a new body starts as soon as any running one finishes. What an
/// item error does is `on_item_failure`.
///
/// A finished item records its appends under its occurrence, with the item as
/// witness (`Context::record_item`), before the map returns. An item an
/// earlier attempt recorded on the same item value is not run again: its
/// recorded appends are used. A record made on another value is ignored and
/// replaced. Inside a called graph nothing is recorded: the call restarts
/// whole (see `SubGraph`).
///
/// At build, `check` refuses a list key that is not a list whose element kind
/// is the item key's kind, checks the limit and the capture updates, then
/// checks the body against the same schema.
pub struct Map {
    pub list: Key,
    pub item: Key,
    pub body: Box<dyn Node>,
    pub max_concurrency: Option<Limit>,
    pub on_item_failure: ItemFailure,
}

/// What a map does when the body of an item returns an error.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ItemFailure {
    /// Every item runs to its end and the finished ones are recorded, then the
    /// map fails with the first error in item order.
    #[default]
    Finish,
    /// No item starts after the first error. Items already running finish and
    /// are recorded, then the map fails with the first error in item order
    /// among the items that ended. Items that never started are not recorded.
    FailFast,
    /// The item yields these updates instead: all appends, forwarded in item
    /// order and recorded like a result, so the map does not fail because of
    /// an item error. `CaptureSource::From` reads the item's own state (the
    /// item key included), `CaptureSource::Reason` is the error's message. A
    /// panic in a body is not captured: it fails the map node.
    Capture(Vec<CaptureUpdate>),
}

impl Node for Map {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let items: Vec<Value> = state.list(&self.list)?.to_vec();
            let width = match &self.max_concurrency {
                Some(limit) => limit.resolve(config)?.get(),
                None => items.len().max(1),
            };
            let mut prepared = Vec::with_capacity(items.len());
            for (index, item) in items.into_iter().enumerate() {
                prepared.push((index, item, item_context(ctx, index)?));
            }
            let stop = AtomicBool::new(false);
            let runs = prepared
                .into_iter()
                .take_while(|_| !stop.load(Ordering::SeqCst))
                .map(|(index, item, item_ctx)| {
                    self.run_item(index, item, state, config, item_ctx, &stop)
                });
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

    fn check(&self, schema: &Schema) -> Result<(), GraphError> {
        let fits = match (schema.state.get(&self.list), schema.state.get(&self.item)) {
            (Some(Kind::List { element }), Some(item)) => element.as_ref() == item,
            _ => false,
        };
        if !fits {
            return Err(GraphError::MapKeyMismatch {
                list: self.list.clone(),
                item: self.item.clone(),
            });
        }
        if let Some(limit) = &self.max_concurrency {
            limit.check(schema)?;
        }
        if let ItemFailure::Capture(captures) = &self.on_item_failure {
            for capture in captures {
                check_capture(schema, capture, true)?;
            }
        }
        self.body.check(schema)
    }
}

impl Map {
    async fn run_item(
        &self,
        index: usize,
        item: Value,
        state: &State,
        config: &Config,
        ctx: Context,
        stop: &AtomicBool,
    ) -> (usize, Result<Vec<Update>, NodeError>) {
        let result = self.item_updates(item, state, config, &ctx).await;
        if result.is_err() && matches!(self.on_item_failure, ItemFailure::FailFast) {
            stop.store(true, Ordering::SeqCst);
        }
        (index, result)
    }

    async fn item_updates(
        &self,
        item: Value,
        state: &State,
        config: &Config,
        ctx: &Context,
    ) -> Result<Vec<Update>, NodeError> {
        if let Some(updates) = ctx.recorded_item(&item) {
            check_appends(state, &updates)?;
            return Ok(updates);
        }
        let derived = state.derive(&self.item, item.clone())?;
        let updates = match self.body.run(&derived, config, ctx).await {
            Ok(updates) => updates,
            Err(error) => match &self.on_item_failure {
                ItemFailure::Capture(captures) => captured(captures, &derived, &error.to_string())?,
                ItemFailure::Finish | ItemFailure::FailFast => return Err(error),
            },
        };
        check_appends(state, &updates)?;
        ctx.record_item(&item, &updates);
        Ok(updates)
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
