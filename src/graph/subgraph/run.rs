use std::collections::BTreeMap;

use crate::error::GraphError;
use crate::graph::context::Context;
use crate::graph::node::{Node, NodeFuture};
use crate::graph::subgraph::{CaptureSource, CaptureUpdate, Input, OnFailure, SubGraph};
use crate::run::{Outcome, channel, run_nested};
use crate::state::{Config, State, Value};
use crate::update::Update;
use crate::value::{EndLabel, Key};

impl Node for SubGraph {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let child_state =
                self.graph
                    .start_state(self.resolve(&self.inputs, state, config)?)?;
            let child_values: BTreeMap<Key, Value> = self
                .resolve(&self.config, state, config)?
                .into_iter()
                .collect();
            let child_config = Config::new(self.graph.schema(), child_values)?;
            // Nobody holds the sender: the nested run can neither pause nor be
            // cancelled from outside. Dropping this future cancels it.
            let (_, mut inbox) = channel();
            let result = run_nested(
                &self.graph,
                &child_config,
                child_state,
                None,
                ctx,
                &mut inbox,
            )
            .await;
            match result {
                Ok(Outcome::Finished { state, end }) => Ok(self.outputs_of(&state, &end)?),
                Ok(Outcome::Paused { .. } | Outcome::Cancelled { .. }) => {
                    Err(GraphError::SubGraphSuspended.into())
                }
                Err(failure) => match &self.on_failure {
                    OnFailure::Propagate => Err(GraphError::SubGraphFailed {
                        source: Box::new(failure.error),
                    }
                    .into()),
                    OnFailure::Capture(captures) => {
                        Ok(captured(captures, state, &failure.error.to_string())?)
                    }
                },
            }
        })
    }
}

impl SubGraph {
    fn resolve(
        &self,
        mappings: &[(Key, Input)],
        state: &State,
        config: &Config,
    ) -> Result<Vec<(Key, Value)>, GraphError> {
        let mut values = Vec::with_capacity(mappings.len());
        for (child_key, source) in mappings {
            let value = match source {
                Input::From(key) => state.get(key)?.clone(),
                Input::Config(key) => config.get(key)?.clone(),
                Input::Const(value) => value.clone(),
            };
            values.push((child_key.clone(), value));
        }
        Ok(values)
    }

    fn outputs_of(&self, state: &State, end: &EndLabel) -> Result<Vec<Update>, GraphError> {
        let mut updates = Vec::with_capacity(self.outputs.len() + 1);
        for (child_key, target) in &self.outputs {
            updates.push(target.update(state.get(child_key)?.clone()));
        }
        if let Some(target) = &self.end_label {
            updates.push(target.update(Value::str(end.as_str())));
        }
        Ok(updates)
    }
}

/// The updates a captured failure makes, `reason` being the child's error
/// message.
fn captured(
    captures: &[CaptureUpdate],
    state: &State,
    reason: &str,
) -> Result<Vec<Update>, GraphError> {
    let mut updates = Vec::with_capacity(captures.len());
    for capture in captures {
        let value = match capture.source() {
            CaptureSource::Const(value) => value.clone(),
            CaptureSource::From(key) => state.get(key)?.clone(),
            CaptureSource::Reason => Value::str(reason),
        };
        updates.push(capture.target().update(value));
    }
    Ok(updates)
}
