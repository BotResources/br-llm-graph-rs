use std::sync::Arc;

use br_llm_messages::{AssistantBlock, Author, Conversation, Entry, Step, StopReason, Text, Turn};
use serde_json::json;

use crate::error::GraphError;
use crate::graph::{
    CheckSite, Context, FnEdge, Graph, GraphBuilder, Limit, Node, NodeFuture, Switch, Target,
};
use crate::react::helpers::{complete, final_step, last_turn_by_author, structured, wire};
use crate::react::llm_node::{LlmNode, Source};
use crate::react::model::{Model, OutputMode, Request, ToolCalls};
use crate::react::react_agent::{declare_limit, declare_switch, round_limit_flag};
use crate::react::react_loop::ReactLoop;
use crate::react::round_limit::RoundLimit;
use crate::react::tool::Tool;
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;
use crate::value::{EndLabel, Key, NodeId};

/// The generator's seat: who writes the answers, with what, and its tools.
pub struct GeneratorSeat {
    pub author: Author,
    pub model: Arc<dyn Model>,
    pub output: OutputMode,
    pub thinking: Option<Switch>,
    pub tools: Vec<Arc<dyn Tool>>,
    /// How the tools are split between tool nodes. Empty: one node, `tools`,
    /// runs them all.
    pub tool_nodes: Vec<(NodeId, Vec<Arc<dyn Tool>>)>,
    pub tool_concurrency: Option<Limit>,
    pub round_limit: Option<RoundLimit>,
}

/// The critic's seat: who judges the answers, with what.
pub struct CriticSeat {
    pub author: Author,
    pub model: Arc<dyn Model>,
    pub thinking: Option<Switch>,
}

/// A generator and a critic writing in one shared history, as a graph.
///
/// Each seat reads from its own perspective: its own turns as assistant
/// messages, the other's as framed messages.
///
/// - Inputs: `conversation` (the shared history the critic reads; a new call
///   holds one user input), `generator_history` (the generator's working
///   history; when empty at entry it starts as a copy of `conversation`),
///   `generator_system` and `critic_system` (the rendered system prompts).
/// - Outputs: `answer` (a conversation holding the generator's last answer,
///   one turn with its final step), `validated`, `generations` (the generator
///   answers in `conversation` since its last user input), `last_critique`
///   (the message of the last rejection, empty when validated),
///   `conversation`, `generator_history` and, when the generator's round limit
///   has a flag, that flag. The run ends with `validated` or `exhausted`.
/// - Configuration: the keys the switches and limits read.
///
/// `generator_history` holds the generator's whole traffic (tool calls and
/// results, answers) and the critiques; `conversation` holds the user input,
/// the answers (final step only) and the critiques. A critique is a turn of
/// the critic holding one text step, the verdict's `message`, appended to
/// both; nothing is appended on validation, and nothing of the critic's
/// reasoning enters a history.
///
/// The entry is routed on how `conversation` ends: a generator answer goes
/// to the critic (a resumed call), anything else to the generator. After an
/// answer, more than `max_critiques` answers end `exhausted` without the
/// critic; otherwise the critic judges it. A valid verdict ends `validated`;
/// a rejection sends the generator back, until `max_critiques` answers were
/// rejected: then the generator answers once more, unassessed, when
/// `final_generation`, else the run ends `exhausted` on the rejected answer.
///
/// With tools, the generation is the `ReactLoop` fragment inline in this
/// graph; each generation opens a new generator turn, so the round limit
/// counts per generation.
pub struct GeneratorCritic {
    pub generator: GeneratorSeat,
    pub critic: CriticSeat,
    pub max_critiques: Limit,
    pub final_generation: bool,
}

/// The names of the graph's keys and nodes.
struct Names {
    conversation: Key,
    generator_history: Key,
    generator_system: Key,
    critic_system: Key,
    answer: Key,
    validated: Key,
    generations: Key,
    last_critique: Key,
    route: NodeId,
    generate: NodeId,
    publish: NodeId,
    critique: NodeId,
    finish: NodeId,
}

impl Names {
    fn new() -> Result<Self, GraphError> {
        Ok(Self {
            conversation: Key::new("conversation")?,
            generator_history: Key::new("generator_history")?,
            generator_system: Key::new("generator_system")?,
            critic_system: Key::new("critic_system")?,
            answer: Key::new("answer")?,
            validated: Key::new("validated")?,
            generations: Key::new("generations")?,
            last_critique: Key::new("last_critique")?,
            route: NodeId::new("route")?,
            generate: NodeId::new("generate")?,
            publish: NodeId::new("publish")?,
            critique: NodeId::new("critique")?,
            finish: NodeId::new("finish")?,
        })
    }
}

/// Where the run goes once the critic has seen an answer, or before.
#[derive(Clone)]
struct Router {
    conversation: Key,
    validated: Key,
    generator: Author,
    max_critiques: Limit,
    final_generation: bool,
    generate: NodeId,
    critique: NodeId,
    finish: NodeId,
}

impl Router {
    /// On entry: a generator answer last goes to the critic, anything else
    /// to the generator.
    fn entry(&self, state: &State, config: &Config) -> Result<Vec<Target>, GraphError> {
        match state.conversation(&self.conversation)?.entries().last() {
            Some(Entry::Turn(turn)) if turn.author() == Some(&self.generator) => {
                self.after_answer(state, config)
            }
            Some(Entry::Turn(_) | Entry::UserInput(_)) | None => {
                Ok(vec![Target::Node(self.generate.clone())])
            }
        }
    }

    fn after_answer(&self, state: &State, config: &Config) -> Result<Vec<Target>, GraphError> {
        let generations = self.generations(state)?;
        let max = self.max_critiques.resolve(config)?.get();
        let next = if generations > max {
            &self.finish
        } else {
            &self.critique
        };
        Ok(vec![Target::Node(next.clone())])
    }

    fn after_critique(&self, state: &State, config: &Config) -> Result<Vec<Target>, GraphError> {
        if state.bool(&self.validated)? {
            return Ok(vec![Target::Node(self.finish.clone())]);
        }
        let generations = self.generations(state)?;
        let max = self.max_critiques.resolve(config)?.get();
        let next = if generations < max || self.final_generation {
            &self.generate
        } else {
            &self.finish
        };
        Ok(vec![Target::Node(next.clone())])
    }

    fn generations(&self, state: &State) -> Result<usize, GraphError> {
        Ok(since_last_input(state.conversation(&self.conversation)?)
            .iter()
            .filter(|entry| authored_by(entry, &self.generator))
            .count())
    }
}

impl GeneratorCritic {
    pub fn new(generator: GeneratorSeat, critic: CriticSeat, max_critiques: Limit) -> Self {
        Self {
            generator,
            critic,
            max_critiques,
            final_generation: true,
        }
    }

    pub fn graph(self) -> Result<Arc<Graph>, GraphError> {
        if self.generator.author == self.critic.author {
            return Err(GraphError::SameAuthor {
                author: self.generator.author,
            });
        }
        let names = Names::new()?;
        let generator = self.generator;
        let critic = self.critic;

        let mut schema = Schema::builder()
            .state(names.conversation.clone(), Kind::Conversation)
            .state(names.generator_history.clone(), Kind::Conversation)
            .state(names.generator_system.clone(), Kind::Str)
            .state(names.critic_system.clone(), Kind::Str)
            .state(names.answer.clone(), Kind::Conversation)
            .state(names.validated.clone(), Kind::Bool)
            .state(names.generations.clone(), Kind::Int)
            .state(names.last_critique.clone(), Kind::Str);
        schema = declare_switch(schema, generator.thinking.as_ref());
        schema = declare_switch(schema, critic.thinking.as_ref());
        schema = declare_limit(schema, Some(&self.max_critiques));
        schema = declare_limit(schema, generator.tool_concurrency.as_ref());
        if let Some(round_limit) = &generator.round_limit {
            schema = declare_limit(schema, Some(&round_limit.max_rounds));
        }
        let mut schema = schema.build();
        let flag = generator.round_limit.as_ref().and_then(round_limit_flag);
        if let Some(flag) = &flag {
            schema.state.entry(flag.clone()).or_insert(Kind::Bool);
        }
        self.max_critiques.check(&schema)?;

        let router = Router {
            conversation: names.conversation.clone(),
            validated: names.validated.clone(),
            generator: generator.author.clone(),
            max_critiques: self.max_critiques,
            final_generation: self.final_generation,
            generate: names.generate.clone(),
            critique: names.critique.clone(),
            finish: names.finish.clone(),
        };

        let mut builder = GraphBuilder::new(schema)
            .entry(names.route.clone())
            .input(names.conversation.clone())
            .input(names.generator_history.clone())
            .input(names.generator_system.clone())
            .input(names.critic_system.clone())
            .output(names.answer.clone())
            .output(names.validated.clone())
            .output(names.generations.clone())
            .output(names.last_critique.clone())
            .output(names.conversation.clone())
            .output(names.generator_history.clone());
        if let Some(flag) = flag {
            builder = builder.output(flag);
        }

        let tool_nodes = if generator.tool_nodes.is_empty() && !generator.tools.is_empty() {
            vec![(NodeId::new("tools")?, generator.tools.clone())]
        } else {
            generator.tool_nodes
        };
        let react = ReactLoop {
            llm: names.generate.clone(),
            tool_nodes,
            after: Target::Node(names.publish.clone()),
            tool_concurrency: generator.tool_concurrency,
            round_limit: generator.round_limit,
        };
        let llm = LlmNode {
            key: names.generator_history.clone(),
            author: generator.author.clone(),
            model: generator.model,
            system: vec![Source::State(names.generator_system.clone())],
            tools: generator.tools,
            enabled: None,
            output: generator.output,
            thinking: generator.thinking,
        };
        let builder = react.add(builder, llm)?;

        let (entry, published, judged) = (router.clone(), router.clone(), router);
        let validated = names.validated.clone();
        let graph = builder
            .node(
                names.route.clone(),
                RouteNode {
                    conversation: names.conversation.clone(),
                    generator_history: names.generator_history.clone(),
                },
            )
            .edge(
                names.route.clone(),
                FnEdge::new(move |state, config| entry.entry(state, config)),
            )
            .node(
                names.publish.clone(),
                PublishNode {
                    conversation: names.conversation.clone(),
                    generator_history: names.generator_history.clone(),
                    author: generator.author.clone(),
                },
            )
            .edge(
                names.publish.clone(),
                FnEdge::new(move |state, config| published.after_answer(state, config)),
            )
            .node(
                names.critique.clone(),
                CritiqueNode {
                    conversation: names.conversation.clone(),
                    generator_history: names.generator_history.clone(),
                    critic_system: names.critic_system.clone(),
                    validated: names.validated.clone(),
                    author: critic.author.clone(),
                    model: critic.model,
                    thinking: critic.thinking,
                },
            )
            .edge(
                names.critique.clone(),
                FnEdge::new(move |state, config| judged.after_critique(state, config)),
            )
            .node(
                names.finish.clone(),
                FinishNode {
                    conversation: names.conversation,
                    answer: names.answer,
                    validated: names.validated,
                    generations: names.generations,
                    last_critique: names.last_critique,
                    generator: generator.author,
                    critic: critic.author,
                },
            )
            .edge(
                names.finish,
                FnEdge::new(move |state, _config| {
                    let label = if state.bool(&validated)? {
                        "validated"
                    } else {
                        "exhausted"
                    };
                    Ok(vec![Target::End(EndLabel::new(label)?)])
                }),
            )
            .build()?;
        Ok(Arc::new(graph))
    }
}

/// The JSON Schema of the critic's verdict. A critic that does not think
/// natively writes its reasoning first, in `thinking`.
pub(crate) fn verdict_schema(native_thinking: bool) -> serde_json::Value {
    if native_thinking {
        json!({
            "type": "object",
            "properties": {
                "is_valid": { "type": "boolean" },
                "message": { "type": "string" }
            },
            "required": ["is_valid", "message"],
            "additionalProperties": false
        })
    } else {
        json!({
            "type": "object",
            "properties": {
                "thinking": { "type": "string" },
                "is_valid": { "type": "boolean" },
                "message": { "type": "string" }
            },
            "required": ["thinking", "is_valid", "message"],
            "additionalProperties": false
        })
    }
}

#[derive(serde::Deserialize)]
struct Verdict {
    is_valid: bool,
    message: String,
}

fn read_verdict(step: &Step) -> Result<Verdict, GraphError> {
    let verdict: Verdict = structured(step)?;
    if !verdict.is_valid && verdict.message.trim().is_empty() {
        return Err(GraphError::Structured {
            message: "a rejecting verdict carries an empty message".to_owned(),
        });
    }
    Ok(verdict)
}

fn since_last_input(conversation: &Conversation) -> &[Entry] {
    let entries = conversation.entries();
    let start = entries
        .iter()
        .rposition(|entry| matches!(entry, Entry::UserInput(_)))
        .map_or(0, |index| index + 1);
    entries.get(start..).unwrap_or_default()
}

fn authored_by(entry: &Entry, author: &Author) -> bool {
    match entry {
        Entry::Turn(turn) => turn.author() == Some(author),
        Entry::UserInput(_) => false,
    }
}

/// Starts `generator_history` from `conversation` when it is empty.
struct RouteNode {
    conversation: Key,
    generator_history: Key,
}

impl Node for RouteNode {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            if !state
                .conversation(&self.generator_history)?
                .entries()
                .is_empty()
            {
                return Ok(Vec::new());
            }
            Ok(vec![Update::Set {
                key: self.generator_history.clone(),
                value: state.get(&self.conversation)?.clone(),
            }])
        })
    }
}

/// Appends the final step of the generator's last turn to `conversation`, as
/// a turn of the generator under the same id.
struct PublishNode {
    conversation: Key,
    generator_history: Key,
    author: Author,
}

impl Node for PublishNode {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let history = state.conversation(&self.generator_history)?;
            let Some(turn) = last_turn_by_author(history, &self.author) else {
                return Ok(Vec::new());
            };
            let Some(step) = final_step(turn) else {
                return Ok(Vec::new());
            };
            Ok(vec![Update::PushTurn {
                key: self.conversation.clone(),
                turn: Turn::new(turn.id().clone(), Some(self.author.clone()), step.clone()),
            }])
        })
    }
}

/// Asks the critic for a verdict on `conversation`. A rejection appends its
/// message, as a turn of the critic, to both histories.
struct CritiqueNode {
    conversation: Key,
    generator_history: Key,
    critic_system: Key,
    validated: Key,
    author: Author,
    model: Arc<dyn Model>,
    thinking: Option<Switch>,
}

impl Node for CritiqueNode {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let thinking = match &self.thinking {
                Some(switch) => Some(switch.resolve(config)?),
                None => None,
            };
            let request = Request {
                system: Some(state.str(&self.critic_system)?.to_owned()),
                messages: wire(state.conversation(&self.conversation)?, &self.author)?,
                tools: Vec::new(),
                tool_calls: ToolCalls::Allowed,
                output: OutputMode::Structured {
                    schema: verdict_schema(thinking == Some(true)),
                },
                thinking,
            };
            let step = complete(self.model.as_ref(), request, ctx, &self.conversation).await?;
            let verdict = read_verdict(&step)?;
            if verdict.is_valid {
                return Ok(vec![Update::Set {
                    key: self.validated.clone(),
                    value: Value::bool(true),
                }]);
            }
            let critique = Step::new(
                vec![AssistantBlock::Text {
                    text: Text::new(verdict.message)?,
                }],
                StopReason::EndTurn,
                None,
                None,
            )?;
            let turn = Turn::new(ctx.ids.turn_id(), Some(self.author.clone()), critique);
            Ok(vec![
                Update::Set {
                    key: self.validated.clone(),
                    value: Value::bool(false),
                },
                Update::PushTurn {
                    key: self.conversation.clone(),
                    turn: turn.clone(),
                },
                Update::PushTurn {
                    key: self.generator_history.clone(),
                    turn,
                },
            ])
        })
    }

    fn check(&self, schema: &Schema, _site: CheckSite) -> Result<(), GraphError> {
        match &self.thinking {
            Some(switch) => switch.check(schema),
            None => Ok(()),
        }
    }
}

/// Sets `answer`, `generations` and `last_critique` from `conversation`.
struct FinishNode {
    conversation: Key,
    answer: Key,
    validated: Key,
    generations: Key,
    last_critique: Key,
    generator: Author,
    critic: Author,
}

impl Node for FinishNode {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let recent = since_last_input(state.conversation(&self.conversation)?);
            let generations = recent
                .iter()
                .filter(|entry| authored_by(entry, &self.generator))
                .count();
            let mut answer = Conversation::new();
            if let Some(Entry::Turn(turn)) = recent
                .iter()
                .rev()
                .find(|entry| authored_by(entry, &self.generator))
            {
                answer.push_turn(turn.clone())?;
            }
            let last_critique = if state.bool(&self.validated)? {
                String::new()
            } else {
                recent
                    .iter()
                    .rev()
                    .find_map(|entry| match entry {
                        Entry::Turn(turn) if turn.author() == Some(&self.critic) => {
                            final_step(turn)
                        }
                        Entry::Turn(_) | Entry::UserInput(_) => None,
                    })
                    .map(|step| {
                        step.text()
                            .map(Text::as_str)
                            .collect::<Vec<_>>()
                            .join("\n\n")
                    })
                    .unwrap_or_default()
            };
            Ok(vec![
                Update::Set {
                    key: self.answer.clone(),
                    value: Value::conversation(answer),
                },
                Update::Set {
                    key: self.generations.clone(),
                    value: Value::int(i64::try_from(generations).unwrap_or(i64::MAX)),
                },
                Update::Set {
                    key: self.last_critique.clone(),
                    value: Value::str(last_critique),
                },
            ])
        })
    }
}
