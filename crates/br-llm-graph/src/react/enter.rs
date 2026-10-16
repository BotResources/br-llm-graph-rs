//! The entry of a ReAct agent: `conversation` started from `opening` in
//! memory, or rehydrated from the agent's thread when it is durable, and
//! the route to the model call or, when the call was answered on entry, to
//! the answer.

use br_llm_messages::Conversation;

use crate::graph::{Context, Edge, FnEdge, Node, NodeFuture, Target};
use crate::react::agent_call::{AgentCall, Position};
use crate::react::rehydrate::{enter_thread, opening_inputs};
use crate::react::thread_store::MessageStatus;
use crate::state::{Config, State, Value};
use crate::update::Update;
use crate::value::{Key, NodeId};

/// The `enter` node of a ReAct agent.
///
/// In memory, `conversation` starts as `opening` and the call writes its
/// first turn. Durable, the agent enters its thread (see `enter_thread`):
/// `conversation` is the thread as it stands, `thread` the id of the thread,
/// and the call goes on from the last of its turns the thread holds: none,
/// or one failed or cancelled (a turn found open is failed first), and it
/// writes the turn of the next rank; one completed, and the call is
/// answered: the agent gives its outputs from the thread without calling the
/// model.
pub(crate) struct EnterNode {
    pub(crate) conversation: Key,
    pub(crate) call: AgentCall,
}

impl Node for EnterNode {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let opening = state.conversation(&self.call.opening)?;
            let Some(binding) = &self.call.thread else {
                return Ok(vec![
                    self.start(opening.clone()),
                    self.call.move_to(Position::Writing(1)),
                ]);
            };
            let inputs = opening_inputs(opening)?;
            let given = state.str(&binding.key)?;
            let entered = enter_thread(ctx, binding, given, &inputs).await?;
            let position = match entered.last() {
                None => Position::Writing(1),
                Some((rank, MessageStatus::Completed)) => Position::Answered(rank),
                Some((rank, _)) => Position::Writing(rank + 1),
            };
            Ok(vec![
                self.start(entered.conversation),
                self.call.move_to(position),
                Update::Set {
                    key: binding.key.clone(),
                    value: Value::str(entered.thread.as_str()),
                },
            ])
        })
    }
}

impl EnterNode {
    fn start(&self, conversation: Conversation) -> Update {
        Update::Set {
            key: self.conversation.clone(),
            value: Value::conversation(conversation),
        }
    }
}

/// The edge of `enter`: to the answer when the call was answered on entry,
/// else to the model call.
pub(crate) fn enter_edge(call: AgentCall, llm: NodeId, answer: NodeId) -> impl Edge + 'static {
    FnEdge::new(move |state, _config| match call.position(state)? {
        Position::Answered(_) => Ok(vec![Target::Node(answer.clone())]),
        Position::Writing(_) => Ok(vec![Target::Node(llm.clone())]),
    })
}
