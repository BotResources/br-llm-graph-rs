//! The bookkeeping nodes of the generator-critic graph: the caller's
//! `prepare` node, the routing node that starts `conversation`, and the
//! outputs written on finishing, with the end label they give.

use std::sync::Arc;

use br_llm_messages::{Conversation, Entry};

use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Edge, FnEdge, Node, NodeFuture, Target};
use crate::react::agent_call::Position;
use crate::react::duo::{Duo, TurnKind};
use crate::react::helpers::{answer_text, final_step};
use crate::react::rehydrate::{enter_thread, opening_inputs};
use crate::react::round_limit::RoundLimit;
use crate::react::turn_clients::TurnClients;
use crate::react::turn_ids::agent_origin;
use crate::state::{Config, Schema, State, Value};
use crate::update::Update;
use crate::value::{EndLabel, Key};

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

/// The `route` node: starts `conversation` and the list of the call's turns.
///
/// In memory, `conversation` starts as `opening`, and the call's turns are
/// the turns of the generator and of the critic since the trigger (the last
/// user input of `opening`; every entry when it has none). Durable, the
/// generator-critic enters its thread (see `enter_thread`): `conversation`
/// is the thread as it stands, `thread` the id of the thread, and the call's
/// turns are those the thread holds under the ids of the call, rank after
/// rank, a turn found open failed and repaired first; the call goes on in
/// the turn of the next rank. Where the run goes is the edge's (see
/// `Router::entry`).
pub(crate) struct RouteNode {
    pub(crate) duo: Duo,
    /// The clients of the generator's turns: entering, the call forgets its
    /// client, so its first round asks the model again.
    pub(crate) clients: Arc<TurnClients>,
}

impl Node for RouteNode {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            self.clients.forget(&agent_origin(ctx));
            let call = &self.duo.call;
            let opening = state.conversation(&call.opening)?;
            let Some(binding) = &call.thread else {
                let ids = self.since_trigger(opening);
                return Ok(vec![
                    self.start(opening.clone()),
                    self.listed(ids),
                    call.move_to(Position::Writing(1)),
                ]);
            };
            let inputs = opening_inputs(opening)?;
            let given = state.str(&binding.key)?;
            let entered = enter_thread(ctx, binding, given, &inputs).await?;
            let ids = entered
                .turns
                .iter()
                .filter(|(_, id, _)| entered.conversation.turn(id).is_some())
                .map(|(_, id, _)| id.as_str().to_owned())
                .collect();
            let position = entered.last().map_or(Position::Writing(1), |(rank, _)| {
                Position::Writing(rank + 1)
            });
            Ok(vec![
                self.start(entered.conversation),
                self.listed(ids),
                call.move_to(position),
                Update::Set {
                    key: binding.key.clone(),
                    value: Value::str(entered.thread.as_str()),
                },
            ])
        })
    }
}

impl RouteNode {
    fn start(&self, conversation: Conversation) -> Update {
        Update::Set {
            key: self.duo.conversation.clone(),
            value: Value::conversation(conversation),
        }
    }

    fn listed(&self, ids: Vec<String>) -> Update {
        Update::Set {
            key: self.duo.turns.clone(),
            value: Value::list(ids.into_iter().map(Value::str).collect()),
        }
    }

    /// The ids of the turns of the generator and of the critic since the
    /// last user input of `opening`.
    fn since_trigger(&self, opening: &Conversation) -> Vec<String> {
        let entries = opening.entries();
        let start = entries
            .iter()
            .rposition(|entry| matches!(entry, Entry::UserInput(_)))
            .map_or(0, |index| index + 1);
        entries
            .get(start..)
            .unwrap_or_default()
            .iter()
            .filter_map(|entry| match entry {
                Entry::Turn(turn) if self.duo.kind(turn).is_some() => {
                    Some(turn.id().as_str().to_owned())
                }
                Entry::Turn(_) | Entry::UserInput(_) => None,
            })
            .collect()
    }
}

/// Sets the outputs from the turns of the call: `answer`, the generator's
/// last answer as the critic sees it (empty when none, or when the round
/// limit cut it); `is_validated`, true when the call ends with an
/// acceptance; `nb_generations`; and the round limit's flag, true when the
/// limit was reached in at least one generation of the call.
pub(crate) struct FinishNode {
    pub(crate) duo: Duo,
    pub(crate) answer: Key,
    pub(crate) validated: Key,
    pub(crate) generations: Key,
    /// The generator's round limit and its flag, when it has one.
    pub(crate) flag: Option<(Key, RoundLimit)>,
}

impl Node for FinishNode {
    fn run<'a>(
        &'a self,
        state: &'a State,
        config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let duo = &self.duo;
            let turns = duo.turns(state)?;
            let answer = match turns.last_generation() {
                Some(turn) if !duo.cut(turn) => {
                    final_step(turn).map(answer_text).unwrap_or_default()
                }
                Some(_) | None => String::new(),
            };
            let validated = matches!(turns.last(), Some((_, TurnKind::Acceptance)));
            let generations = turns.generations();
            let mut updates = vec![
                Update::Set {
                    key: self.answer.clone(),
                    value: Value::str(answer),
                },
                Update::Set {
                    key: self.validated.clone(),
                    value: Value::bool(validated),
                },
                Update::Set {
                    key: self.generations.clone(),
                    value: Value::int(i64::try_from(generations).unwrap_or(i64::MAX)),
                },
            ];
            if let Some((flag, limit)) = &self.flag {
                let mut reached = state.bool(flag)?;
                for turn in turns.generation_turns() {
                    reached = reached || limit.reached_in_turn(turn, config)?;
                }
                if reached {
                    updates.push(Update::Set {
                        key: flag.clone(),
                        value: Value::bool(true),
                    });
                }
            }
            Ok(updates)
        })
    }
}

/// The edge of `finish`: the run ends with `validated` when `is_validated`
/// (the key `validated`) is true, else `exhausted`.
pub(crate) fn finish_edge(validated: Key) -> impl Edge + 'static {
    FnEdge::new(move |state, _config| {
        let label = if state.bool(&validated)? {
            "validated"
        } else {
            "exhausted"
        };
        Ok(vec![Target::End(EndLabel::new(label)?)])
    })
}
