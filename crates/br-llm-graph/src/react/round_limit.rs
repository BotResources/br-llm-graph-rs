use br_llm_messages::{
    AssistantBlock, Author, Conversation, Step, StopReason, Text, ToolResult, ToolResultBlock,
    Turn, TurnItem, TurnState,
};

use crate::error::GraphError;
use crate::graph::{Always, Context, GraphBuilder, Limit, Node, NodeFuture, Target};
use crate::react::helpers::{last_turn_by_author, pending_calls};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;
use crate::value::{Key, NodeId};

/// Bounds the tool rounds of one agent turn. A round is one reply that calls
/// tools plus the execution of all its calls; the count starts again with
/// each new turn of the agent. `on_limit` says what happens when a reply asks
/// for tools after the last allowed round.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundLimit {
    pub max_rounds: Limit,
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
    Continue { node: NodeId, flag: Key },
    /// Each call is answered the same way and the turn is closed with a step
    /// whose stop reason is `other(tool_round_limit)`, without calling the
    /// model.
    End { node: NodeId, flag: Key },
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
            OnLimit::Continue { node, .. } if !answered_by_limit(conversation, author) => {
                Ok(vec![Target::Node(node.clone())])
            }
            OnLimit::Error | OnLimit::Continue { .. } => Err(GraphError::ToolLimitReached {
                node: llm.clone(),
                max_rounds: max,
            }),
        }
    }

    pub(crate) fn add_limit_node(
        &self,
        builder: GraphBuilder,
        key: Key,
        author: Author,
        llm: &NodeId,
        after: &Target,
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
                },
            )
            .edge(node.clone(), Always(next))
    }
}

/// The stop reason of the step that closes a turn cut by `OnLimit::End`.
const LIMIT_REACHED: &str = "tool_round_limit";

/// Whether `step` is the step `OnLimit::End` closes a turn with.
pub(crate) fn closes_on_limit(step: &Step) -> bool {
    matches!(step.stop_reason(), StopReason::Other { reason } if reason == LIMIT_REACHED)
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

/// Whether the limit node has answered calls in `author`'s open turn: one of
/// its tool results is the limit's refusal.
fn answered_by_limit(conversation: &Conversation, author: &Author) -> bool {
    open_turn(conversation, author).is_some_and(|turn| {
        turn.items().iter().any(|item| match item {
            TurnItem::ToolResults(results) => results.results().iter().any(is_refusal),
            TurnItem::Step(_) => false,
        })
    })
}

fn open_turn<'a>(conversation: &'a Conversation, author: &Author) -> Option<&'a Turn> {
    last_turn_by_author(conversation, author)
        .filter(|turn| !matches!(turn.state(), TurnState::Finished { .. }))
}

/// The first and last parts of the error result the limit node answers a
/// call with; the limit, a number, stands between them.
const REFUSAL_HEAD: &str = "Tool round limit reached (";
const REFUSAL_TAIL: &str = "): this call was not executed. Reply without calling tools.";

/// The error result's text the limit node answers a call with.
fn refusal(max: usize) -> String {
    format!("{REFUSAL_HEAD}{max}{REFUSAL_TAIL}")
}

/// Whether `result` is the limit node's answer to a call: an error result
/// whose one text is the limit's refusal, whatever limit it names.
fn is_refusal(result: &ToolResult) -> bool {
    result.is_error
        && matches!(result.content.as_slice(), [ToolResultBlock::Text { text }] if text
            .as_str()
            .strip_prefix(REFUSAL_HEAD)
            .and_then(|rest| rest.strip_suffix(REFUSAL_TAIL))
            .is_some_and(|max| max.parse::<usize>().is_ok()))
}

struct LimitNode {
    key: Key,
    author: Author,
    max_rounds: Limit,
    flag: Key,
    end: bool,
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
            let mut updates = Vec::new();
            for call in pending_calls(state, &self.key, &self.author)? {
                updates.push(Update::PushResult {
                    key: self.key.clone(),
                    turn: turn.id().clone(),
                    result: ToolResult::new(
                        call.id.clone(),
                        call.name.clone(),
                        vec![ToolResultBlock::text(refusal.clone())],
                        true,
                    ),
                });
            }
            updates.push(Update::Set {
                key: self.flag.clone(),
                value: Value::bool(true),
            });
            if self.end {
                let text = Text::new(format!("Tool round limit reached ({max}): no final reply."))?;
                let step = Step::new(
                    vec![AssistantBlock::Text { text }],
                    StopReason::Other {
                        reason: LIMIT_REACHED.to_owned(),
                    },
                    None,
                    None,
                )?;
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
