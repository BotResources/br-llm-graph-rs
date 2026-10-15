use std::sync::Arc;

use br_llm_clients_core::{Client, ClientRequest, OutputMode, ToolSpec};
use br_llm_messages::{Author, Text, Turn, TurnState};

use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Node, NodeFuture};
use crate::react::helpers::{
    check_enabled, complete, enabled_names, last_turn_by_author, system_text, wire,
};
use crate::react::source::Source;
use crate::react::tool::Tool;
use crate::state::{Config, Schema, State};
use crate::update::Update;
use crate::value::Key;

/// The model-call step of the ReAct loop: calls `client` for `author` on the
/// conversation `key` and adds the reply.
///
/// The request holds the texts of the `system` sources, in order, each list
/// item a text, joined by a blank line (no text, or a blank one, gives no
/// system text); the conversation rendered for `author`; the declared tools,
/// filtered by the names `enabled` holds when set (names matching no tool are
/// ignored); and `output`. The reply is added as a new step of the author's
/// last turn when that turn awaits a step, else as a new turn of the author
/// with a new id.
///
/// At build, the node checks its `enabled` key (`EnabledKeyMismatch`) and the
/// keys its `system` sources read (`SourceKeyMismatch`).
pub(crate) struct LlmNode {
    pub(crate) key: Key,
    pub(crate) author: Author,
    pub(crate) client: Arc<dyn Client>,
    pub(crate) system: Vec<Source>,
    pub(crate) tools: Vec<Arc<dyn Tool>>,
    /// A state key holding a list of tool names: when set, only the tools it
    /// names are offered to the model. Checked at build
    /// (`EnabledKeyMismatch`).
    pub(crate) enabled: Option<Key>,
    pub(crate) output: OutputMode,
}

impl Node for LlmNode {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let messages = wire(state.conversation(&self.key)?, &self.author)?;
            let request = ClientRequest::new(
                self.system_text(state, config)?,
                messages,
                self.declared_tools(state)?,
                self.output.clone(),
            )
            .map_err(GraphError::Request)?;
            let step = complete(self.client.as_ref(), &request, ctx, &self.key).await?;

            let conversation = state.conversation(&self.key)?;
            let update = match last_turn_by_author(conversation, &self.author) {
                Some(turn) if matches!(turn.state(), TurnState::AwaitingStep) => Update::PushStep {
                    key: self.key.clone(),
                    turn: turn.id().clone(),
                    step,
                },
                _ => Update::PushTurn {
                    key: self.key.clone(),
                    turn: Turn::new(ctx.ids.turn_id(), Some(self.author.clone()), step),
                },
            };
            Ok(vec![update])
        })
    }

    fn check(&self, schema: &Schema, _site: CheckSite) -> Result<(), GraphError> {
        check_enabled(schema, self.enabled.as_ref())?;
        for source in &self.system {
            source.check(schema)?;
        }
        Ok(())
    }
}

impl LlmNode {
    fn system_text(&self, state: &State, config: &Config) -> Result<Option<Text>, GraphError> {
        let mut parts: Vec<String> = Vec::new();
        for source in &self.system {
            parts.extend(source.texts(state, config)?);
        }
        Ok(system_text(parts.join("\n\n")))
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
