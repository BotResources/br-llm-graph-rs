use std::sync::Arc;

use br_llm_clients_core::{ClientRequest, OutputMode, ToolSpec};
use br_llm_messages::{
    Author, Conversation, StopReason, Turn, TurnId, TurnItem, TurnState, UserInput,
};

use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Node, NodeFuture};
use crate::react::agent_call::{AgentCall, Position};
use crate::react::given::{AgentFacts, AgentNode, History, Model, Prompt};
use crate::react::helpers::{check_enabled, enabled_names, last_turn_by_author, system_text, wire};
use crate::react::sink::complete_in;
use crate::react::thread_store::TurnEnd;
use crate::react::tool::Tool;
use crate::react::turn_ids::{id_request, next_rank_in_memory, trigger};
use crate::react::turn_writer::TurnWriter;
use crate::state::{Config, Schema, State};
use crate::update::Update;
use crate::value::Key;

/// The model-call step of the ReAct loop of the agent `N`: calls the client
/// its `model` gives for this round, for `author`, on the conversation `key`,
/// and adds the reply.
///
/// The request holds the text `system` gives (a blank one gives no system
/// text); the messages `history` gives, by default the conversation rendered
/// for `author`; the declared tools, filtered by the names `enabled` holds
/// when set (names matching no tool are ignored); and `output_mode`. The
/// functions read the agent's context: the state, the configuration, the
/// conversation and the round, the rounds already answered in the turn the
/// reply goes to (0 for a new turn). The reply is added as a new step of the
/// turn the agent writes when that turn awaits a step, else as a new turn of
/// the author under an id from the id source (see `target`).
///
/// For a durable agent the turn is written to its thread first: a new turn
/// is opened under its id, the sink pushes each stream event into it, and a
/// reply that awaits no tool result completes it; only then is the reply
/// applied to the conversation.
///
/// At build, the node checks its `enabled` key (`EnabledKeyMismatch`) and the
/// keys its functions declare (`UnknownKey`, `KindMismatch`).
pub(crate) struct LlmNode<N: AgentNode> {
    pub(crate) key: Key,
    pub(crate) author: Author,
    pub(crate) model: Model<N>,
    pub(crate) system: Prompt<N>,
    /// `None`: the conversation rendered for `author`.
    pub(crate) history: Option<History<N>>,
    pub(crate) tools: Vec<Arc<dyn Tool>>,
    /// A state key holding a list of tool names: when set, only the tools it
    /// names are offered to the model. Checked at build
    /// (`EnabledKeyMismatch`).
    pub(crate) enabled: Option<Key>,
    pub(crate) output_mode: OutputMode,
    /// The call of the agent: the turn the reply goes to and, for a durable
    /// agent, the thread it streams into. `None`: the conversation alone
    /// says where the reply goes, kept in memory.
    pub(crate) call: Option<AgentCall>,
}

impl<N: AgentNode> Node for LlmNode<N> {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let conversation = state.conversation(&self.key)?;
            let target = self.target(state, ctx, conversation)?;
            let facts = N::from_agent(AgentFacts {
                conversation,
                round: match &target {
                    Target::Step(turn) => answered_rounds(turn),
                    Target::Open { .. } => 0,
                },
            });
            let messages = match &self.history {
                Some(history) => history.give(state, config, facts)?,
                None => wire(conversation, &self.author)?,
            };
            let request = ClientRequest::new(
                system_text(self.system.give(state, config, facts)?),
                messages,
                self.declared_tools(state)?,
                self.output_mode.clone(),
            )
            .map_err(GraphError::Request)?;
            let client = self.model.give(state, config, facts)?;

            let turn_id = match &target {
                Target::Step(turn) => turn.id().clone(),
                Target::Open { id, .. } => id.clone(),
            };
            let writer = match self.call.as_ref().and_then(|call| call.thread.as_ref()) {
                Some(binding) => Some(TurnWriter {
                    store: binding.store.as_ref(),
                    thread: binding.thread(state)?,
                    turn: turn_id.clone(),
                }),
                None => None,
            };
            if let (Some(writer), Target::Open { .. }) = (&writer, &target) {
                writer.open(&self.author).await?;
            }
            let step =
                complete_in(client.as_ref(), &request, ctx, &self.key, writer.as_ref()).await?;
            if let Some(writer) = &writer
                && !matches!(step.stop_reason(), StopReason::AwaitingToolResults)
            {
                writer.end(TurnEnd::Completed).await?;
            }

            Ok(match target {
                Target::Step(_) => vec![Update::PushStep {
                    key: self.key.clone(),
                    turn: turn_id,
                    step,
                }],
                Target::Open { moved, .. } => {
                    let turn = Turn::new(turn_id, Some(self.author.clone()), step);
                    let mut updates = vec![Update::PushTurn {
                        key: self.key.clone(),
                        turn,
                    }];
                    if let (Some(call), Some(position)) = (&self.call, moved) {
                        updates.push(call.move_to(position));
                    }
                    updates
                }
            })
        })
    }

    fn check(&self, schema: &Schema, _site: CheckSite) -> Result<(), GraphError> {
        check_enabled(schema, self.enabled.as_ref())?;
        self.model.reads().check(schema)?;
        self.system.reads().check(schema)?;
        if let Some(history) = &self.history {
            history.reads().check(schema)?;
        }
        Ok(())
    }
}

/// Where the reply of a model call goes.
enum Target<'a> {
    /// A new step of this turn, which awaits one.
    Step(&'a Turn),
    /// The first step of a new turn under `id`; the call moves to `moved`
    /// when set.
    Open { id: TurnId, moved: Option<Position> },
}

/// The rounds `turn` has answered: its tool results items.
fn answered_rounds(turn: &Turn) -> usize {
    turn.items()
        .iter()
        .filter(|item| matches!(item, TurnItem::ToolResults(_)))
        .count()
}

impl<N: AgentNode> LlmNode<N> {
    /// Where the reply goes. With the agent's call: the turn it writes, while
    /// that turn awaits a step; a new one when it is not in the conversation
    /// yet, or the turn of the next rank once it is finished. Without (the
    /// generator-critic until it keeps its call): the author's last turn
    /// while it awaits a step, else a new turn, ranked one past the turns
    /// that follow the trigger.
    fn target<'a>(
        &self,
        state: &State,
        ctx: &Context,
        conversation: &'a Conversation,
    ) -> Result<Target<'a>, GraphError> {
        let Some(call) = &self.call else {
            return Ok(match last_turn_by_author(conversation, &self.author) {
                Some(turn) if matches!(turn.state(), TurnState::AwaitingStep) => Target::Step(turn),
                _ => {
                    let trigger = trigger(conversation).map(UserInput::id);
                    let rank = next_rank_in_memory(conversation);
                    Target::Open {
                        id: ctx.ids.turn_id(&id_request(ctx, trigger, rank)),
                        moved: None,
                    }
                }
            });
        };
        let rank = match call.position(state)? {
            Position::Writing(rank) | Position::Answered(rank) => rank,
        };
        let id = call.turn_id(state, ctx, rank)?;
        Ok(match conversation.turn(&id) {
            Some(turn) if matches!(turn.state(), TurnState::AwaitingStep) => Target::Step(turn),
            Some(_) => Target::Open {
                id: call.turn_id(state, ctx, rank + 1)?,
                moved: Some(Position::Writing(rank + 1)),
            },
            None => Target::Open { id, moved: None },
        })
    }

    fn declared_tools(&self, state: &State) -> Result<Vec<ToolSpec>, GraphError> {
        let specs = self.tools.iter().map(|tool| tool.spec());
        let Some(key) = &self.enabled else {
            return Ok(specs.cloned().collect());
        };
        let names = enabled_names(state, key)?;
        Ok(specs
            .filter(|spec| names.contains(&spec.name().as_str()))
            .cloned()
            .collect())
    }
}
