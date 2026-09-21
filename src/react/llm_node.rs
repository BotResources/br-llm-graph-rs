use std::sync::Arc;

use br_llm_messages::{Author, Turn, TurnState};

use crate::error::GraphError;
use crate::graph::{Context, Node, NodeFuture};
use crate::react::helpers::{complete, last_turn_by_author, wire};
use crate::react::model::{Model, OutputMode, Request, ToolSpec};
use crate::react::tool::Tool;
use crate::state::{Config, State, Value};
use crate::update::Update;
use crate::value::Key;

pub enum Source {
    Config(Key),
    State(Key),
}

pub struct LlmNode {
    pub key: Key,
    pub author: Author,
    pub model: Arc<dyn Model>,
    pub system: Vec<Source>,
    pub tools: Vec<Arc<dyn Tool>>,
    pub enabled: Option<Key>,
    pub output: OutputMode,
}

impl Node for LlmNode {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let system = build_system(self, state, config)?;
            let messages = wire(state.conversation(&self.key)?, &self.author)?;
            let tools = declared_tools(self, state)?;
            let request = Request {
                system,
                messages,
                tools,
                output: self.output.clone(),
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
        let value = match source {
            Source::Config(key) => config.value(key)?,
            Source::State(key) => state.value(key)?,
        };
        append_strings(source_key(source), value, &mut parts)?;
    }
    if parts.is_empty() {
        Ok(None)
    } else {
        Ok(Some(parts.join("\n\n")))
    }
}

fn source_key(source: &Source) -> &Key {
    match source {
        Source::Config(key) | Source::State(key) => key,
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
                            expected: crate::state::Kind::Str,
                            found: other.tag(),
                        });
                    }
                }
            }
            Ok(())
        }
        other => Err(GraphError::KindMismatch {
            key: key.clone(),
            expected: crate::state::Kind::Str,
            found: other.tag(),
        }),
    }
}

fn declared_tools(node: &LlmNode, state: &State) -> Result<Vec<ToolSpec>, GraphError> {
    let enabled = match &node.enabled {
        None => {
            return Ok(node.tools.iter().map(|tool| tool.spec()).collect());
        }
        Some(key) => state.list(key)?,
    };
    let mut names: Vec<&str> = Vec::new();
    for value in enabled {
        match value {
            Value::Str(text) => names.push(text),
            _ => continue,
        }
    }
    Ok(node
        .tools
        .iter()
        .map(|tool| tool.spec())
        .filter(|spec| names.contains(&spec.name.as_str()))
        .collect())
}
