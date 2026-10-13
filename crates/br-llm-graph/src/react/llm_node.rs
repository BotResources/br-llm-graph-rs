use std::sync::Arc;

use br_llm_messages::{Author, Turn, TurnState};

use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Node, NodeFuture, Switch};
use crate::react::helpers::{check_enabled, complete, enabled_names, last_turn_by_author, wire};
use crate::react::model::{Model, OutputMode, Request, ToolCalls, ToolSpec};
use crate::react::tool::Tool;
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;
use crate::value::Key;

/// A text computed from the state and the configuration: pure and
/// synchronous.
pub type SourceFn = Arc<dyn Fn(&State, &Config) -> Result<String, GraphError> + Send + Sync>;

/// Where a text comes from.
#[derive(Clone)]
pub enum Source {
    /// A string, or a list of strings, read from a configuration key.
    Config(Key),
    /// A string, or a list of strings, read from a state key.
    State(Key),
    /// A text fixed at build.
    Const(String),
    /// A text computed at each model call from the state and the
    /// configuration. The function is pure and synchronous: no I/O, anything
    /// fetched belongs in a node that writes it to the state. It reads only
    /// keys the graph declares otherwise.
    Fn(SourceFn),
}

impl Source {
    /// The text this source gives on this state and configuration. A list of
    /// strings gives its items separated by a blank line.
    pub fn resolve(&self, state: &State, config: &Config) -> Result<String, GraphError> {
        Ok(self.texts(state, config)?.join("\n\n"))
    }

    /// A `State` key must be a declared state key, a `Config` key a declared
    /// configuration key, holding a string or a list of strings
    /// (`SourceKeyMismatch`).
    pub(crate) fn check(&self, schema: &Schema) -> Result<(), GraphError> {
        let (key, kind) = match self {
            Source::State(key) => (key, schema.state.get(key)),
            Source::Config(key) => (key, schema.config.get(key)),
            Source::Const(_) | Source::Fn(_) => return Ok(()),
        };
        match kind {
            Some(Kind::Str) => Ok(()),
            Some(Kind::List { element }) if **element == Kind::Str => Ok(()),
            Some(_) | None => Err(GraphError::SourceKeyMismatch { key: key.clone() }),
        }
    }

    /// The texts this source gives: one, or one per item of a list.
    pub(crate) fn texts(&self, state: &State, config: &Config) -> Result<Vec<String>, GraphError> {
        let mut parts = Vec::new();
        match self {
            Source::Config(key) => append_strings(key, config.get(key)?, &mut parts)?,
            Source::State(key) => append_strings(key, state.get(key)?, &mut parts)?,
            Source::Const(text) => parts.push(text.clone()),
            Source::Fn(text) => parts.push(text(state, config)?),
        }
        Ok(parts)
    }
}

/// Calls a model for `author` on the conversation `key` and adds the reply.
///
/// At build, the node checks its `enabled` key (`EnabledKeyMismatch`), the
/// keys its `system` sources read (`SourceKeyMismatch`) and its `thinking`
/// switch (`SwitchKeyMismatch`).
pub(crate) struct LlmNode {
    pub(crate) key: Key,
    pub(crate) author: Author,
    pub(crate) model: Arc<dyn Model>,
    pub(crate) system: Vec<Source>,
    pub(crate) tools: Vec<Arc<dyn Tool>>,
    /// A state key holding a list of tool names: when set, only the tools it
    /// names are offered to the model. Checked at build
    /// (`EnabledKeyMismatch`).
    pub(crate) enabled: Option<Key>,
    pub(crate) output: OutputMode,
    /// Native thinking, resolved at each call and sent as
    /// `Request::thinking`; `None` leaves the provider's default.
    pub(crate) thinking: Option<Switch>,
}

impl Node for LlmNode {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        self.step(state, config, ctx, ToolCalls::Allowed)
    }

    fn check(&self, schema: &Schema, _site: CheckSite) -> Result<(), GraphError> {
        check_enabled(schema, self.enabled.as_ref())?;
        for source in &self.system {
            source.check(schema)?;
        }
        match &self.thinking {
            Some(switch) => switch.check(schema),
            None => Ok(()),
        }
    }
}

impl LlmNode {
    pub(crate) fn step<'a>(
        &'a self,
        state: &'a State,
        config: &'a Config,
        ctx: &'a Context,
        tool_calls: ToolCalls,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let system = build_system(self, state, config)?;
            let messages = wire(state.conversation(&self.key)?, &self.author)?;
            let tools = declared_tools(self, state)?;
            let thinking = match &self.thinking {
                Some(switch) => Some(switch.resolve(config)?),
                None => None,
            };
            let request = Request {
                system,
                messages,
                tools,
                tool_calls,
                output: self.output.clone(),
                thinking,
            };
            let step = complete(self.model.as_ref(), request, ctx, &self.key).await?;

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
}

fn build_system(
    node: &LlmNode,
    state: &State,
    config: &Config,
) -> Result<Option<String>, GraphError> {
    let mut parts: Vec<String> = Vec::new();
    for source in &node.system {
        parts.extend(source.texts(state, config)?);
    }
    if parts.is_empty() {
        Ok(None)
    } else {
        Ok(Some(parts.join("\n\n")))
    }
}

fn append_strings(key: &Key, value: &Value, parts: &mut Vec<String>) -> Result<(), GraphError> {
    match value {
        Value::Str(text) => {
            parts.push(text.clone());
            Ok(())
        }
        Value::List(items) => {
            for item in items {
                match item {
                    Value::Str(text) => parts.push(text.clone()),
                    other => {
                        return Err(GraphError::KindMismatch {
                            key: key.clone(),
                            expected: Kind::Str,
                            found: other.tag(),
                        });
                    }
                }
            }
            Ok(())
        }
        other => Err(GraphError::KindMismatch {
            key: key.clone(),
            expected: Kind::Str,
            found: other.tag(),
        }),
    }
}

fn declared_tools(node: &LlmNode, state: &State) -> Result<Vec<ToolSpec>, GraphError> {
    let names = match &node.enabled {
        None => {
            return Ok(node.tools.iter().map(|tool| tool.spec()).collect());
        }
        Some(key) => enabled_names(state, key)?,
    };
    Ok(node
        .tools
        .iter()
        .map(|tool| tool.spec())
        .filter(|spec| names.contains(&spec.name.as_str()))
        .collect())
}
