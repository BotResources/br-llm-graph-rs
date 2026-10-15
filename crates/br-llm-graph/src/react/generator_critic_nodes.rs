//! The bookkeeping nodes of the generator-critic graph: the caller's
//! `prepare` node, the routing node's copy of the history, the publishing of
//! an answer and the outputs written on finishing.

use br_llm_messages::{Author, Turn};

use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Node, NodeFuture};
use crate::react::generator_critic_routing::{authored_by, last_answer, since_last_input};
use crate::react::helpers::{answer_text, final_step, last_turn_by_author};
use crate::react::round_limit::closes_on_limit;
use crate::state::{Config, Schema, State, Value};
use crate::update::Update;
use crate::value::Key;

/// The caller's `prepare` node, run as it is.
pub(crate) struct Prepare(pub(crate) Box<dyn Node>);

impl Node for Prepare {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        self.0.run(state, config, ctx)
    }

    fn check(&self, schema: &Schema, site: CheckSite) -> Result<(), GraphError> {
        self.0.check(schema, site)
    }
}

/// Starts `generator_history` from `conversation` when it is empty.
pub(crate) struct RouteNode {
    pub(crate) conversation: Key,
    pub(crate) generator_history: Key,
}

impl Node for RouteNode {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            if !state
                .conversation(&self.generator_history)?
                .entries()
                .is_empty()
            {
                return Ok(Vec::new());
            }
            Ok(vec![Update::Set {
                key: self.generator_history.clone(),
                value: state.get(&self.conversation)?.clone(),
            }])
        })
    }
}

/// Appends the final step of the generator's last turn to `conversation`, as
/// a turn of the generator under the same id.
pub(crate) struct PublishNode {
    pub(crate) conversation: Key,
    pub(crate) generator_history: Key,
    pub(crate) author: Author,
}

impl Node for PublishNode {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let history = state.conversation(&self.generator_history)?;
            let Some(turn) = last_turn_by_author(history, &self.author) else {
                return Ok(Vec::new());
            };
            let Some(step) = final_step(turn) else {
                return Ok(Vec::new());
            };
            Ok(vec![Update::PushTurn {
                key: self.conversation.clone(),
                turn: Turn::new(turn.id().clone(), Some(self.author.clone()), step.clone()),
            }])
        })
    }
}

/// Sets `answer` and `nb_generations` from `conversation`.
pub(crate) struct FinishNode {
    pub(crate) conversation: Key,
    pub(crate) answer: Key,
    pub(crate) generations: Key,
    pub(crate) generator: Author,
}

impl Node for FinishNode {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let conversation = state.conversation(&self.conversation)?;
            let generations = since_last_input(conversation)
                .iter()
                .filter(|entry| authored_by(entry, &self.generator))
                .count();
            let answer = match last_answer(conversation, &self.generator) {
                Some((_, step)) if !closes_on_limit(step) => answer_text(step),
                Some(_) | None => String::new(),
            };
            Ok(vec![
                Update::Set {
                    key: self.answer.clone(),
                    value: Value::str(answer),
                },
                Update::Set {
                    key: self.generations.clone(),
                    value: Value::int(i64::try_from(generations).unwrap_or(i64::MAX)),
                },
            ])
        })
    }
}
