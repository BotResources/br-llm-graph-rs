use std::sync::Arc;

use br_llm_messages::{Author, Conversation, Turn};

use crate::error::GraphError;
use crate::graph::{
    Always, Context, Count, Graph, GraphBuilder, Limit, Node, NodeFuture, Switch, Target,
};
use crate::react::helpers::{final_step, last_turn_by_author};
use crate::react::llm_node::{LlmNode, Source};
use crate::react::model::{Model, OutputMode};
use crate::react::react_loop::ReactLoop;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::tool::Tool;
use crate::state::{Config, Kind, Schema, SchemaBuilder, State, Value};
use crate::update::Update;
use crate::value::{EndLabel, Key, NodeId};

/// A ReAct agent as a graph, to call with `SubGraph` or run on its own.
///
/// The graph is the `ReactLoop` fragment on the key `history`, followed by a
/// node that reads the reply; it ends with `done`.
///
/// - Inputs: `history` (conversation), the agent's chat history: a new call is
///   a history holding one user input, a resumed call the history the host
///   saved. The loop always enters through the model call. Each
///   `Source::State` key of `system` is a string input too, and so is the
///   `enabled` key, a list of strings, when set.
/// - Outputs: `history` (the conversation after the run), `answer`
///   (conversation, the agent's last turn with its final step only, empty if
///   none), `reply` (string, the text of that step, empty if none or for a
///   structured reply) and,
///   when the round limit's `OnLimit` is `Continue` or `End`, its `flag`
///   (bool, true when the limit was reached).
/// - Configuration: each `Source::Config` key of `system` (string), the
///   `thinking` switch, the tool concurrency and the round limit when they
///   read a configuration key.
///
/// With `enabled`, only the tools the key names are offered to the model, and
/// a call naming another tool is answered with an error result, never
/// executed. A key the graph already has is refused (`KeyCollision`).
///
/// `output` is sent on every model call; with `OutputMode::Structured`, the
/// final step carries the structured block and reaches the caller through
/// `answer`.
///
/// The round limit counts the rounds of the agent's open turn, as in
/// `ReactLoop`.
pub struct ReactAgent {
    pub author: Author,
    pub model: Arc<dyn Model>,
    pub system: Vec<Source>,
    pub output: OutputMode,
    pub tools: Vec<Arc<dyn Tool>>,
    /// How the tools are split between tool nodes. Empty: one node, `tools`,
    /// runs them all.
    pub tool_nodes: Vec<(NodeId, Vec<Arc<dyn Tool>>)>,
    pub tool_concurrency: Option<Limit>,
    pub round_limit: Option<RoundLimit>,
    pub thinking: Option<Switch>,
    /// A state key holding the names of the tools offered on this run.
    pub enabled: Option<Key>,
}

impl ReactAgent {
    pub fn graph(self) -> Result<Arc<Graph>, GraphError> {
        let history = Key::new("history")?;
        let reply = Key::new("reply")?;
        let answer = Key::new("answer")?;
        let llm = NodeId::new("llm")?;
        let reply_node = NodeId::new("reply")?;

        let mut inputs = vec![history.clone()];
        let mut schema = Schema::builder();
        for source in &self.system {
            schema = match source {
                Source::Config(key) => schema.config(key.clone(), Kind::Str),
                Source::State(key) => {
                    inputs.push(key.clone());
                    schema.state(key.clone(), Kind::Str)
                }
            };
        }
        schema = schema
            .state(history.clone(), Kind::Conversation)
            .state(reply.clone(), Kind::Str)
            .state(answer.clone(), Kind::Conversation);
        schema = declare_switch(schema, self.thinking.as_ref());
        schema = declare_limit(schema, self.tool_concurrency.as_ref());
        let flag = self.round_limit.as_ref().and_then(round_limit_flag);
        if let Some(round_limit) = &self.round_limit {
            schema = declare_limit(schema, Some(&round_limit.max_rounds));
        }
        let mut schema = schema.build();
        if let Some(flag) = &flag {
            schema.state.entry(flag.clone()).or_insert(Kind::Bool);
        }
        if let Some(key) = &self.enabled {
            declare_input(&mut schema, key, Kind::list(Kind::Str))?;
            inputs.push(key.clone());
        }

        let tool_nodes = if self.tool_nodes.is_empty() && !self.tools.is_empty() {
            vec![(NodeId::new("tools")?, self.tools.clone())]
        } else {
            self.tool_nodes
        };
        let react = ReactLoop {
            llm: llm.clone(),
            tool_nodes,
            after: Target::Node(reply_node.clone()),
            tool_concurrency: self.tool_concurrency,
            round_limit: self.round_limit,
        };
        let llm_node = LlmNode {
            key: history.clone(),
            author: self.author.clone(),
            model: self.model,
            system: self.system,
            tools: self.tools,
            enabled: self.enabled,
            output: self.output,
            thinking: self.thinking,
        };

        let mut builder = GraphBuilder::new(schema).entry(llm);
        for key in inputs {
            builder = builder.input(key);
        }
        builder = builder
            .output(history.clone())
            .output(answer.clone())
            .output(reply.clone());
        if let Some(flag) = flag {
            builder = builder.output(flag);
        }
        let builder = react.add(builder, llm_node)?;
        let graph = builder
            .node(
                reply_node.clone(),
                ReplyNode {
                    history,
                    reply,
                    answer,
                    author: self.author,
                },
            )
            .edge(reply_node, Always(Target::End(EndLabel::new("done")?)))
            .build()?;
        Ok(Arc::new(graph))
    }
}

/// Declares the configuration key a switch reads, as a bool.
pub(crate) fn declare_switch(schema: SchemaBuilder, switch: Option<&Switch>) -> SchemaBuilder {
    match switch {
        Some(Switch::Config(key)) => schema.config(key.clone(), Kind::Bool),
        Some(Switch::Fixed(_)) | None => schema,
    }
}

/// Declares the configuration key a limit reads, as an int.
pub(crate) fn declare_limit(schema: SchemaBuilder, limit: Option<&Limit>) -> SchemaBuilder {
    match limit {
        Some(Limit::Config(key)) => schema.config(key.clone(), Kind::Int),
        Some(Limit::Fixed(_)) | None => schema,
    }
}

/// Declares a state key the graph takes from its caller. A key the graph
/// already has is refused.
pub(crate) fn declare_input(schema: &mut Schema, key: &Key, kind: Kind) -> Result<(), GraphError> {
    if schema.state.contains_key(key) {
        return Err(GraphError::KeyCollision { key: key.clone() });
    }
    schema.state.insert(key.clone(), kind);
    Ok(())
}

/// Declares the configuration key a count reads, as an int.
pub(crate) fn declare_count(schema: SchemaBuilder, count: &Count) -> SchemaBuilder {
    match count {
        Count::Config(key) => schema.config(key.clone(), Kind::Int),
        Count::Fixed(_) => schema,
    }
}

/// The state key a round limit sets when it is reached, if any.
pub(crate) fn round_limit_flag(round_limit: &RoundLimit) -> Option<Key> {
    match &round_limit.on_limit {
        OnLimit::Error => None,
        OnLimit::Continue { flag, .. } | OnLimit::End { flag, .. } => Some(flag.clone()),
    }
}

/// Sets `answer` to the agent's last turn with its final step only, and
/// `reply` to the text of that step.
struct ReplyNode {
    history: Key,
    reply: Key,
    answer: Key,
    author: Author,
}

impl Node for ReplyNode {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let conversation = state.conversation(&self.history)?;
            let mut answer = Conversation::new();
            let mut text = String::new();
            if let Some(turn) = last_turn_by_author(conversation, &self.author)
                && let Some(step) = final_step(turn)
            {
                text = step
                    .text()
                    .map(|text| text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                answer.push_turn(Turn::new(
                    turn.id().clone(),
                    Some(self.author.clone()),
                    step.clone(),
                ))?;
            }
            Ok(vec![
                Update::Set {
                    key: self.answer.clone(),
                    value: Value::conversation(answer),
                },
                Update::Set {
                    key: self.reply.clone(),
                    value: Value::str(text),
                },
            ])
        })
    }
}
