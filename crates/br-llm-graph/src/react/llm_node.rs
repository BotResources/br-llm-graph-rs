use std::sync::Arc;

use br_llm_clients_core::{ClientRequest, OutputMode, ToolSpec};
use br_llm_messages::{Author, Turn, TurnState, UserInput};

use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Node, NodeFuture};
use crate::react::given::{AgentFacts, AgentNode, History, Model, Prompt};
use crate::react::helpers::{check_enabled, enabled_names, last_turn_by_author, system_text, wire};
use crate::react::round_limit::completed_rounds;
use crate::react::sink::complete;
use crate::react::tool::Tool;
use crate::react::turn_ids::{id_request, next_rank_in_memory, trigger};
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
/// conversation and the round, the rounds already answered in the author's
/// open turn. The reply is added as a new step of the author's last turn
/// when that turn awaits a step, else as a new turn of the author with a new
/// id.
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
}

impl<N: AgentNode> Node for LlmNode<N> {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let conversation = state.conversation(&self.key)?;
            let facts = N::from_agent(AgentFacts {
                conversation,
                round: completed_rounds(conversation, &self.author),
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
            let step = complete(client.as_ref(), &request, ctx, &self.key).await?;

            let update = match last_turn_by_author(conversation, &self.author) {
                Some(turn) if matches!(turn.state(), TurnState::AwaitingStep) => Update::PushStep {
                    key: self.key.clone(),
                    turn: turn.id().clone(),
                    step,
                },
                _ => {
                    let trigger = trigger(conversation).map(UserInput::id);
                    let rank = next_rank_in_memory(conversation);
                    let id = ctx.ids.turn_id(&id_request(ctx, trigger, rank));
                    Update::PushTurn {
                        key: self.key.clone(),
                        turn: Turn::new(id, Some(self.author.clone()), step),
                    }
                }
            };
            Ok(vec![update])
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

impl<N: AgentNode> LlmNode<N> {
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
