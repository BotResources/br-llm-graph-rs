use std::sync::Arc;

use br_llm_clients_core::{Client, OutputMode, ToolSpec};
use br_llm_messages::{Author, TurnId};

use crate::describe::NodeDescription;
use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Count, Limit, Node, NodeError, NodeFuture};
use crate::react::agent_call::AgentCall;
use crate::react::duo::Duo;
use crate::react::given::{History, LoopNode, Model, Prompt, SeatCount};
use crate::react::helpers::{check_enabled, enabled_names};
use crate::react::llm_node_reply::{Ask, Replied};
use crate::react::llm_node_target::Target;
use crate::react::replies::Unusable;
use crate::react::round_limit::RoundLimit;
use crate::react::tool::Tool;
use crate::react::turn_clients::TurnClients;
use crate::state::{Config, Schema, State};
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
/// conversation, the round, the rounds already answered in the turn the
/// reply goes to (0 for a new turn), and the attempt. The reply is added as
/// a new step of the turn the agent writes when that turn awaits a step,
/// else as a new turn of the author under an id from the id source (see
/// `target`): in memory, the author's last turn; durable, the turn of the
/// call's rank.
///
/// A reply that cannot be used (see `judged`) is asked for again while
/// `reply_retries` are left (see `reply`), the attempt one higher: the model
/// is asked again for it, and the client it gives is kept for the turn's
/// other rounds. It is never added to the conversation and counts against
/// no round limit. When no retry is left, the node fails with `Model` or
/// `Structured`.
///
/// For a durable agent the turn is written to its thread first: a new turn
/// is opened under its id, the sink pushes each stream event into it (with a
/// structured output mode, once the step is checked), the store is given
/// the finished step, and a reply that awaits no tool result completes it;
/// only then is the reply applied to the conversation.
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
    /// At most that many tool calls of the loop at once, across its tool
    /// nodes: the one source the loop reads (`ReactLoop::add`) and the
    /// description writes.
    pub(crate) tool_concurrency: Option<Limit>,
    /// The bound on the tool rounds of one turn: the one source the loop
    /// reads (`ReactLoop::add`) and the description writes.
    pub(crate) round_limit: Option<RoundLimit>,
    /// The times a reply that cannot be used is asked for again: the one
    /// source the node reads and the description writes. The agent's entry
    /// checks it with its other bounds.
    pub(crate) reply_retries: Count,
    /// The reminder of a reply asked for again. `None`: `REMINDER`.
    pub(crate) reminder: Option<Prompt<N>>,
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
            let attempts = self.reply_retries.resolve(config)?.saturating_add(1);
            // The state as a resumed run would find it, once a turn the
            // client's refused reply had streamed into was failed.
            let mut reentered: Option<State> = None;
            let mut opened: Option<TurnId> = None;
            let mut refused: Option<Unusable> = None;
            let mut attempt = 1;
            loop {
                let ask = Ask {
                    attempt,
                    refused: refused.as_ref(),
                    retry_left: attempt < attempts,
                };
                let now = reentered.as_ref().unwrap_or(state);
                match self.reply(now, config, ctx, ask, &mut opened).await? {
                    Replied::Used(updates) => {
                        return Ok(self.reentered(reentered.as_ref(), updates)?);
                    }
                    Replied::Unusable { unusable, .. } if attempt >= attempts => {
                        return Err(unusable.into_error().into());
                    }
                    Replied::Unusable {
                        unusable,
                        reentered: again,
                    } => {
                        if again.is_some() {
                            reentered = again;
                        }
                        refused = Some(unusable);
                        attempt += 1;
                    }
                }
            }
        })
    }

    fn check(&self, schema: &Schema, _site: CheckSite) -> Result<(), GraphError> {
        check_enabled(schema, self.enabled.as_ref())?;
        self.model.reads().check(schema)?;
        self.system.reads().check(schema)?;
        if let Some(history) = &self.history {
            history.reads().check(schema)?;
        }
        if let Some(reminder) = &self.reminder {
            reminder.reads().check(schema)?;
        }
        Ok(())
    }

    /// `model_call`: the seat's author, how its model, system, history and
    /// reminder are given, its tools, its enabled key, its bounds and its
    /// reply retries.
    fn describe(&self) -> NodeDescription {
        self.description()
    }
}

impl<N: LoopNode> LlmNode<N> {
    /// The client called for this model call: on a first attempt, the one
    /// `model` gave for the turn `turn`, asked at the turn's first round (in
    /// this call, for a turn it continues) for the context of that round,
    /// and kept in the call's run for the turn's other rounds, so a turn is
    /// run by one client (see `TurnClients`); for a reply asked for again,
    /// the one `model` gives for the attempt, kept in its place.
    pub(crate) fn client<'a>(
        &'a self,
        state: &'a State,
        config: &'a Config,
        ctx: &Context,
        facts: N::Facts<'a>,
        turn: &TurnId,
        attempt: usize,
    ) -> Result<Arc<dyn Client>, NodeError> {
        let clients = TurnClients::of(ctx);
        if attempt == 1
            && let Some(kept) = clients.kept(ctx, turn)
        {
            return Ok(kept);
        }
        let client = self.model.give(state, config, facts)?;
        clients.keep(ctx, turn.clone(), client.clone());
        Ok(client)
    }

    /// Where the call of a generator-critic stands for the generator's
    /// context: the number of the generation the reply goes to, counted
    /// among the generator's turns of the call. Nothing for a ReAct agent.
    pub(crate) fn count(
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

    pub(crate) fn declared_tools(&self, state: &State) -> Result<Vec<ToolSpec>, GraphError> {
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
