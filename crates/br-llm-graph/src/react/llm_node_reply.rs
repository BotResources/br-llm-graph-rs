//! One attempt of an agent's model call: the request built for the attempt,
//! the reply judged, written to the thread when it can be used, and the
//! updates that add it to the conversation.

use br_llm_clients_core::{ClientRequest, OutputMode};
use br_llm_messages::{StopReason, Turn, TurnId};

use crate::error::GraphError;
use crate::graph::{Context, NodeError};
use crate::react::given::LoopNode;
use crate::react::helpers::{system_text, wire};
use crate::react::llm_node::LlmNode;
use crate::react::llm_node_target::{Target, answered_rounds};
use crate::react::replies::{Unusable, judged, reminded, reminder};
use crate::react::sink::{ToThread, stream};
use crate::react::thread_store::TurnEnd;
use crate::react::turn_clients::TurnClients;
use crate::react::turn_writer::TurnWriter;
use crate::state::{Config, State};
use crate::update::Update;

/// What one attempt asks: its number, from 1; why the reply before it could
/// not be used, for its reminder; and whether a retry is left after it.
pub(crate) struct Ask<'a> {
    pub(crate) attempt: usize,
    pub(crate) unusable: Option<&'a Unusable>,
    pub(crate) retry_left: bool,
}

/// What one attempt gave.
pub(crate) enum Replied {
    /// The reply was used: the updates that add it to the conversation.
    Used(Vec<Update>),
    /// The reply cannot be used: why; and, when the client refused it after
    /// some of its events reached the thread and a retry is left, the state
    /// a resumed run would find once its turn is failed (see `reenter`).
    Unusable {
        unusable: Unusable,
        reentered: Option<State>,
    },
}

impl<N: LoopNode> LlmNode<N> {
    /// One attempt of the model call on `state`, as `ask` says.
    ///
    /// The request is built for the attempt's context; after a reply that
    /// could not be used, it is followed by the reminder. A durable agent
    /// opens the turn of a new reply once, at its first attempt (`opened`
    /// keeps it). Its sink pushes each event into the turn as it comes, or,
    /// with a structured output mode, holds them until the step is checked:
    /// a reply asked for again never reaches the thread. A reply the client
    /// refuses after some of its events reached the thread fails its turn
    /// when a retry is left, and the retry is made in the turn of the next
    /// rank (see `reenter`). A reply that can be used is written (its events
    /// pushed when held, the store given the finished step, the turn
    /// completed when it awaits no tool result), then given as updates.
    pub(crate) async fn reply(
        &self,
        state: &State,
        config: &Config,
        ctx: &Context,
        ask: Ask<'_>,
        opened: &mut Option<TurnId>,
    ) -> Result<Replied, NodeError> {
        let conversation = state.conversation(&self.key)?;
        let target = self.target(state, ctx, conversation)?;
        let round = match &target {
            Target::Step(turn) => answered_rounds(turn),
            Target::Open { .. } => 0,
        };
        let count = self.count(state, config, &target)?;
        let rank = self.rank(state, &target)?;
        let facts = N::loop_facts(
            conversation,
            round,
            rank.saturating_sub(1),
            count,
            ask.attempt,
        );
        let mut messages = match &self.history {
            Some(history) => history.give(state, config, facts)?,
            None => wire(conversation, &self.author)?,
        };
        if let Some(unusable) = ask.unusable {
            let text = reminder(
                self.reminder.as_ref(),
                state,
                config,
                facts,
                &unusable.reason(),
            )?;
            messages = reminded(messages, text);
        }
        let request = ClientRequest::new(
            system_text(self.system.give(state, config, facts)?),
            messages,
            self.declared_tools(state)?,
            self.output_mode.clone(),
        )
        .map_err(GraphError::Request)?;
        let turn_id = match &target {
            Target::Step(turn) => turn.id().clone(),
            Target::Open { id, .. } => id.clone(),
        };
        let client = self.client(state, config, ctx, facts, &turn_id, ask.attempt)?;

        let writer = match &self.call.thread {
            Some(binding) => Some(TurnWriter {
                store: binding.store.as_ref(),
                thread: binding.thread(state)?,
                turn: turn_id.clone(),
            }),
            None => None,
        };
        if let (Some(writer), Target::Open { .. }) = (&writer, &target)
            && opened.as_ref() != Some(&turn_id)
        {
            writer.open(&self.author).await?;
            *opened = Some(turn_id.clone());
        }
        let structured = matches!(self.output_mode, OutputMode::Structured(_));
        let to_thread = writer.as_ref().map(|writer| {
            if structured {
                ToThread::Held
            } else {
                ToThread::Live(writer)
            }
        });
        let streamed = stream(client.as_ref(), &request, ctx, &self.key, to_thread).await;
        let step = match judged(streamed.result, &self.output_mode)? {
            Ok(step) => step,
            Err(unusable) => {
                let reentered = match &writer {
                    Some(writer) if streamed.reached && ask.retry_left => {
                        Some(self.reenter(state, writer, rank).await?)
                    }
                    Some(_) | None => None,
                };
                return Ok(Replied::Unusable {
                    unusable,
                    reentered,
                });
            }
        };
        let ends = !matches!(step.stop_reason(), StopReason::AwaitingToolResults);
        if let Some(writer) = &writer {
            writer.push_events(&streamed.held).await?;
            writer.finish_step(&step).await?;
        }
        if ends {
            TurnClients::of(ctx).forget(ctx);
        }
        if let Some(writer) = &writer
            && ends
        {
            writer.end(TurnEnd::Completed).await?;
        }

        Ok(Replied::Used(match target {
            Target::Step(_) => vec![Update::PushStep {
                key: self.key.clone(),
                turn: turn_id,
                step,
            }],
            Target::Open { moved, .. } => {
                let turn = Turn::new(turn_id, Some(self.author.clone()), step);
                let added = self.call.add_turn(turn.id());
                let mut updates = vec![Update::PushTurn {
                    key: self.key.clone(),
                    turn,
                }];
                updates.extend(added);
                if let Some(position) = moved {
                    updates.push(self.call.move_to(position));
                }
                updates
            }
        }))
    }
}
