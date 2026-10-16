use br_llm_messages::{
    AssistantBlock, Author, Conversation, Step, StopReason, Text, ToolResult, ToolResultBlock,
    Turn, TurnItem, TurnState,
};

use crate::error::GraphError;
use crate::graph::{Always, Context, GraphBuilder, Limit, Node, NodeFuture, Target};
use crate::react::agent_call::ThreadBinding;
use crate::react::helpers::{last_turn_by_author, pending_calls};
use crate::react::round_limit_marks::{LIMIT_REACHED, closing, reached_in, refusal};
use crate::react::thread_store::TurnEnd;
use crate::react::turn_writer::TurnWriter;
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;
use crate::value::{Key, NodeId};

/// Bounds the tool rounds of one agent turn. A round is one reply that calls
/// tools plus the execution of all its calls; the count starts again with
/// each new turn of the agent. `on_limit` says what happens when a reply asks
/// for tools after the last allowed round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundLimit {
    /// The rounds an agent turn may answer.
    pub max_rounds: Limit,
    /// What a reply asking for tools after the last allowed round does.
    pub on_limit: OnLimit,
}

/// What a reply asking for tools after the last allowed round does: a reply
/// of a turn that already holds `max_rounds` answered rounds or more, as a
/// resumed turn may when the limit is read lower than before. `node` names
/// the node added to do it; `flag`, a bool state key, is set to true.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnLimit {
    /// The run fails with `ToolLimitReached`.
    Error,
    /// Each call is answered with an error result saying it was not
    /// executed, and the model is called once more, its tools still
    /// declared (br-llm-clients-core has no setting that forbids tool calls);
    /// a reply that still calls tools once the node has answered in this turn
    /// fails the run with `ToolLimitReached`.
    ///
    /// The node has answered in this turn when a round of the open turn at
    /// 0-based index `max_rounds` or more holds only its refusals (error
    /// results whose one text is the refusal, whatever limit it names): tool
    /// nodes never run past the limit, so no other node answers those rounds,
    /// and a lookalike a tool writes in an earlier round does not count. The
    /// one residual: after the limit is read lower, a round a tool node
    /// answered under the higher limit sits past the new one, and a tool
    /// whose every result there was exactly the refusal would read as the
    /// node's answer.
    Continue {
        /// The id of the node added to answer the calls.
        node: NodeId,
        /// The bool state key set to true.
        flag: Key,
    },
    /// Each call is answered the same way and the turn is closed with a step
    /// whose stop reason is `other(tool_round_limit)`, without calling the
    /// model.
    End {
        /// The id of the node added to answer the calls and close the turn.
        node: NodeId,
        /// The bool state key set to true.
        flag: Key,
    },
}

impl RoundLimit {
    pub(crate) fn check(&self, schema: &Schema) -> Result<(), GraphError> {
        self.max_rounds.check(schema)?;
        match &self.on_limit {
            OnLimit::Error => Ok(()),
            OnLimit::Continue { flag, .. } | OnLimit::End { flag, .. } => {
                match schema.state.get(flag) {
                    Some(Kind::Bool) => Ok(()),
                    Some(_) | None => Err(GraphError::FlagKeyMismatch { key: flag.clone() }),
                }
            }
        }
    }

    /// Where a reply of `author`'s open turn that asks for tools after the
    /// last allowed round goes: `End` always to its node; `Continue` to its
    /// node until that node has answered in this turn, then the run fails, as
    /// `Error` does.
    pub(crate) fn reached(
        &self,
        llm: &NodeId,
        conversation: &Conversation,
        author: &Author,
        max: usize,
    ) -> Result<Vec<Target>, GraphError> {
        match &self.on_limit {
            OnLimit::End { node, .. } => Ok(vec![Target::Node(node.clone())]),
            OnLimit::Continue { node, .. } if !answered_by_limit(conversation, author, max) => {
                Ok(vec![Target::Node(node.clone())])
            }
            OnLimit::Error | OnLimit::Continue { .. } => Err(GraphError::ToolLimitReached {
                node: llm.clone(),
                max_rounds: max,
            }),
        }
    }

    /// Whether this limit was reached in `turn`, its limit read from
    /// `config`: never for `Error`, which fails the run instead.
    pub(crate) fn reached_in_turn(&self, turn: &Turn, config: &Config) -> Result<bool, GraphError> {
        match &self.on_limit {
            OnLimit::Error => Ok(false),
            OnLimit::Continue { .. } | OnLimit::End { .. } => {
                Ok(reached_in(turn, self.max_rounds.resolve(config)?.get()))
            }
        }
    }

    /// Adds the limit node, when `on_limit` has one, writing to `thread` for
    /// a durable agent.
    pub(crate) fn add_limit_node(
        &self,
        builder: GraphBuilder,
        key: Key,
        author: Author,
        (llm, after): (&NodeId, &Target),
        thread: Option<ThreadBinding>,
    ) -> GraphBuilder {
        let (node, flag, end, next) = match &self.on_limit {
            OnLimit::Error => return builder,
            OnLimit::Continue { node, flag } => (node, flag, false, Target::Node(llm.clone())),
            OnLimit::End { node, flag } => (node, flag, true, after.clone()),
        };
        builder
            .node(
                node.clone(),
                LimitNode {
                    key,
                    author,
                    max_rounds: self.max_rounds.clone(),
                    flag: flag.clone(),
                    end,
                    thread,
                },
            )
            .edge(node.clone(), Always(next))
    }
}

/// The rounds answered in `author`'s open turn: none when the last turn of
/// `author` is finished, or when there is none.
pub(crate) fn completed_rounds(conversation: &Conversation, author: &Author) -> usize {
    open_turn(conversation, author).map_or(0, |turn| {
        turn.items()
            .iter()
            .filter(|item| matches!(item, TurnItem::ToolResults(_)))
            .count()
    })
}

/// Whether the limit node has answered calls in `author`'s open turn under a
/// limit of `max` (see `reached_in`).
fn answered_by_limit(conversation: &Conversation, author: &Author, max: usize) -> bool {
    open_turn(conversation, author).is_some_and(|turn| reached_in(turn, max))
}

fn open_turn<'a>(conversation: &'a Conversation, author: &Author) -> Option<&'a Turn> {
    last_turn_by_author(conversation, author)
        .filter(|turn| !matches!(turn.state(), TurnState::Finished { .. }))
}

/// Answers each pending call of the agent's open turn with the limit's
/// refusal, sets the flag and, with `End`, closes the turn. For a durable
/// agent the refusals and the closing step are written to the thread first,
/// the closing step streamed as a client would stream it, and the turn
/// completed.
struct LimitNode {
    key: Key,
    author: Author,
    max_rounds: Limit,
    flag: Key,
    end: bool,
    thread: Option<ThreadBinding>,
}

impl Node for LimitNode {
    fn run<'a>(
        &'a self,
        state: &'a State,
        config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let max = self.max_rounds.resolve(config)?.get();
            let conversation = state.conversation(&self.key)?;
            let Some(turn) = last_turn_by_author(conversation, &self.author) else {
                return Ok(Vec::new());
            };
            let refusal = Text::new(refusal(max))?;
            let results: Vec<ToolResult> = pending_calls(state, &self.key, &self.author)?
                .into_iter()
                .map(|call| {
                    ToolResult::new(
                        call.id.clone(),
                        call.name.clone(),
                        vec![ToolResultBlock::text(refusal.clone())],
                        true,
                    )
                })
                .collect();
            let closing = if self.end {
                Some(Text::new(closing(max))?)
            } else {
                None
            };
            let stop_reason = StopReason::Other {
                reason: LIMIT_REACHED.to_owned(),
            };
            if let Some(binding) = &self.thread {
                let writer = TurnWriter {
                    store: binding.store.as_ref(),
                    thread: binding.thread(state)?,
                    turn: turn.id().clone(),
                };
                if !results.is_empty() {
                    writer.push_results(turn.items().len(), &results).await?;
                }
                if let Some(text) = &closing {
                    writer.push_text_step(text, stop_reason.clone()).await?;
                    writer.end(TurnEnd::Completed).await?;
                }
            }
            let mut updates: Vec<Update> = results
                .into_iter()
                .map(|result| Update::PushResult {
                    key: self.key.clone(),
                    turn: turn.id().clone(),
                    result,
                })
                .collect();
            updates.push(Update::Set {
                key: self.flag.clone(),
                value: Value::bool(true),
            });
            if let Some(text) = closing {
                let step = Step::new(vec![AssistantBlock::Text { text }], stop_reason, None, None)?;
                updates.push(Update::PushStep {
                    key: self.key.clone(),
                    turn: turn.id().clone(),
                    step,
                });
            }
            Ok(updates)
        })
    }
}
