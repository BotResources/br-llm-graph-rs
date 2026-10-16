use std::sync::Arc;

use br_llm_clients_core::{ClientRequest, OutputMode, ToolSpec};
use br_llm_messages::{Author, StopReason, Turn, TurnId};

use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Node, NodeFuture};
use crate::react::agent_call::AgentCall;
use crate::react::duo::Duo;
use crate::react::given::{History, LoopNode, Model, Prompt, SeatCount};
use crate::react::helpers::{check_enabled, enabled_names, system_text, wire};
use crate::react::llm_node_target::{Target, answered_rounds};
use crate::react::sink::complete_in;
use crate::react::thread_store::TurnEnd;
use crate::react::tool::Tool;
use crate::react::turn_clients::TurnClients;
use crate::react::turn_writer::TurnWriter;
use crate::state::{Config, Schema, State};
use crate::update::Update;
use crate::value::Key;

/// The model-call step of the ReAct loop of the agent `N`: calls the client
/// its `model` gives for the turn (asked at the turn's first round, kept for
/// its other rounds), for `author`, on the conversation `key`,
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
/// the author under an id from the id source (see `target`): in memory, the
/// author's last turn; durable, the turn of the call's rank.
///
/// For a durable agent the turn is written to its thread first: a new turn
/// is opened under its id, the sink pushes each stream event into it, and a
/// reply that awaits no tool result completes it; only then is the reply
/// applied to the conversation.
///
/// At build, the node checks its `enabled` key (`EnabledKeyMismatch`) and the
/// keys its functions declare (`UnknownKey`, `KindMismatch`).
pub(crate) struct LlmNode<N: LoopNode> {
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
    /// agent, the thread it streams into.
    pub(crate) call: AgentCall,
    /// For the generator of a generator-critic: where the call stands, for
    /// the generator's context.
    pub(crate) seat: Option<Duo>,
}

impl<N: LoopNode> Node for LlmNode<N> {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let conversation = state.conversation(&self.key)?;
            let target = self.target(state, ctx, conversation)?;
            let round = match &target {
                Target::Step(turn) => answered_rounds(turn),
                Target::Open { .. } => 0,
            };
            let count = self.count(state, config, &target)?;
            let facts = N::loop_facts(conversation, round, self.turn_index(state, &target)?, count);
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
            let turn_id = match &target {
                Target::Step(turn) => turn.id().clone(),
                Target::Open { id, .. } => id.clone(),
            };
            let client = self.client(state, config, ctx, facts, &turn_id)?;

            let writer = match &self.call.thread {
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
            if !matches!(step.stop_reason(), StopReason::AwaitingToolResults) {
                TurnClients::of(ctx).forget(ctx);
            }
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

impl<N: LoopNode> LlmNode<N> {
    /// The client called for this model call: the one `model` gave for the
    /// turn `turn`, asked at the turn's first round (in this call, for a turn
    /// it continues) for the context of that round, and kept in the call's
    /// run for the turn's other rounds, so a turn is run by one client (see
    /// `TurnClients`).
    fn client<'a>(
        &'a self,
        state: &'a State,
        config: &'a Config,
        ctx: &Context,
        facts: N::Facts<'a>,
        turn: &TurnId,
    ) -> Result<Arc<dyn br_llm_clients_core::Client>, crate::graph::NodeError> {
        let clients = TurnClients::of(ctx);
        if let Some(kept) = clients.kept(ctx, turn) {
            return Ok(kept);
        }
        let client = self.model.give(state, config, facts)?;
        clients.keep(ctx, turn.clone(), client.clone());
        Ok(client)
    }

    /// Where the call of a generator-critic stands for the generator's
    /// context: the number of the generation the reply goes to, counted
    /// among the generator's turns of the call. Nothing for a ReAct agent.
    fn count(
        &self,
        state: &State,
        config: &Config,
        target: &Target<'_>,
    ) -> Result<SeatCount, GraphError> {
        let Some(duo) = &self.seat else {
            return Ok(SeatCount::default());
        };
        let turns = duo.turns(state)?;
        let writing = match target {
            Target::Step(turn) => Some(turn.id()),
            Target::Open { .. } => None,
        };
        duo.count(config, &turns, turns.generations_before(writing) + 1)
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
