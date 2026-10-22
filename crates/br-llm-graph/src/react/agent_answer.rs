//! The answer of a ReAct agent: the text of the final step of the call's
//! turn, given back as `answer`.

use br_llm_messages::Author;

use crate::graph::{Context, Node, NodeFuture};
use crate::react::agent_call::{AgentCall, Position};
use crate::react::helpers::{answer_text, final_step, last_turn_by_author};
use crate::react::round_count::continued;
use crate::react::round_limit::RoundLimit;
use crate::state::{Config, State, Value};
use crate::update::Update;
use crate::value::Key;

/// Sets `answer` to the text of the final step of the call's turn, empty
/// when there is none: the turn the call wrote, the agent's last; or, for a
/// call answered on entry, the completed turn of its rank, whatever turns of
/// the agent the thread holds after it. A call answered on entry also sets
/// the round limit's flag when the limit was reached in that turn, as the
/// run that wrote it did.
pub(crate) struct AnswerNode {
    pub(crate) conversation: Key,
    pub(crate) answer: Key,
    pub(crate) author: Author,
    pub(crate) call: AgentCall,
    /// The round limit and its flag, when it has one.
    pub(crate) flag: Option<(Key, RoundLimit)>,
}

impl Node for AnswerNode {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let conversation = state.conversation(&self.conversation)?;
            let answered = match self.call.position(state)? {
                Position::Answered(rank) => Some(self.call.turn_id(state, ctx, rank)?),
                Position::Writing(_) => None,
            };
            let turn = match &answered {
                Some(id) => conversation.turn(id),
                None => last_turn_by_author(conversation, &self.author),
            };
            let answer = turn
                .and_then(final_step)
                .map(answer_text)
                .unwrap_or_default();
            let mut updates = vec![Update::Set {
                key: self.answer.clone(),
                value: Value::str(answer),
            }];
            if let (Some((flag, limit)), Some(turn), Some(_)) = (&self.flag, turn, &answered)
                && limit.reached_in_turn(
                    &continued(conversation, turn, &self.call.failed_turns(state)?),
                    config,
                )?
            {
                updates.push(Update::Set {
                    key: flag.clone(),
                    value: Value::bool(true),
                });
            }
            Ok(updates)
        })
    }
}
