use std::sync::Arc;

use br_llm_messages::Author;

use crate::error::GraphError;
use crate::graph::{Always, Context, Graph, GraphBuilder, Limit, Node, NodeFuture, Switch, Target};
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
///   `Source::State` key of `system` is a string input too.
/// - Outputs: `history` (the conversation after the run), `reply` (string,
///   the text of the last step of the agent's last turn, empty if none) and,
///   when the round limit's `OnLimit` is `Continue` or `End`, its `flag`
///   (bool, true when the limit was reached).
/// - Configuration: each `Source::Config` key of `system` (string), the
///   `thinking` switch, the tool concurrency and the round limit when they
///   read a configuration key.
///
/// The round limit counts the rounds of the agent's open turn, as in
/// `ReactLoop`.
pub struct ReactAgent {
    pub author: Author,
    pub model: Arc<dyn Model>,
    pub system: Vec<Source>,
    pub tools: Vec<Arc<dyn Tool>>,
    /// How the tools are split between tool nodes. Empty: one node, `tools`,
    /// runs them all.
    pub tool_nodes: Vec<(NodeId, Vec<Arc<dyn Tool>>)>,
    pub tool_concurrency: Option<Limit>,
    pub round_limit: Option<RoundLimit>,
    pub thinking: Option<Switch>,
}

impl ReactAgent {
    pub fn graph(self) -> Result<Arc<Graph>, GraphError> {
        let history = Key::new("history")?;
        let reply = Key::new("reply")?;
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
            .state(reply.clone(), Kind::Str);
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
            enabled: None,
            output: OutputMode::Text,
            thinking: self.thinking,
        };

        let mut builder = GraphBuilder::new(schema).entry(llm);
        for key in inputs {
            builder = builder.input(key);
        }
        builder = builder.output(history.clone()).output(reply.clone());
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

/// The state key a round limit sets when it is reached, if any.
pub(crate) fn round_limit_flag(round_limit: &RoundLimit) -> Option<Key> {
    match &round_limit.on_limit {
        OnLimit::Error => None,
        OnLimit::Continue { flag, .. } | OnLimit::End { flag, .. } => Some(flag.clone()),
    }
}

/// Sets `reply` to the text of the last step of the agent's last turn.
struct ReplyNode {
    history: Key,
    reply: Key,
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
            let text = last_turn_by_author(conversation, &self.author)
                .and_then(final_step)
                .map(|step| {
                    step.text()
                        .map(|text| text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n\n")
                })
                .unwrap_or_default();
            Ok(vec![Update::Set {
                key: self.reply.clone(),
                value: Value::str(text),
            }])
        })
    }
}
