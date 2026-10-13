use std::sync::Arc;

use br_llm_messages::{
    AssistantBlock, Author, Conversation, Entry, Step, StopReason, Text, Turn, WireMessage,
    WireUserBlock,
};

use crate::error::GraphError;
use crate::graph::{
    Always, CheckSite, Context, Count, FnEdge, Graph, GraphBuilder, Limit, Node, NodeFuture,
    Switch, Target,
};
use crate::react::helpers::{answer_text, complete, final_step, last_turn_by_author, wire};
use crate::react::llm_node::{LlmNode, Source};
use crate::react::model::{Model, OutputMode, Request, ToolCalls};
use crate::react::react_agent::{
    declare_count, declare_input, declare_limit, declare_sources, declare_switch, round_limit_flag,
};
use crate::react::react_loop::ReactLoop;
use crate::react::round_limit::{RoundLimit, closes_on_limit};
use crate::react::tool::Tool;
use crate::react::verdict::{Verdict, VerdictForm, VerdictOverride};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;
use crate::value::{EndLabel, Key, NodeId};

/// The generator's seat: who writes the answers, with what, and its tools.
pub struct GeneratorSeat {
    pub author: Author,
    pub model: Arc<dyn Model>,
    /// The generator's system text.
    pub system: Source,
    pub output: OutputMode,
    pub thinking: Option<Switch>,
    pub tools: Vec<Arc<dyn Tool>>,
    /// How the tools are split between tool nodes. Empty: one node, `tools`,
    /// runs them all.
    pub tool_nodes: Vec<(NodeId, Vec<Arc<dyn Tool>>)>,
    /// At most that many tool calls run at once, across all the tool nodes
    /// together. `None`: every call at once.
    pub tool_concurrency: Option<Limit>,
    pub round_limit: Option<RoundLimit>,
    /// A state key holding the names of the tools offered on this run: an
    /// input of the graph, a list of strings. A call naming another tool is
    /// answered with an error result, never executed.
    pub enabled: Option<Key>,
}

/// The critic's seat: who judges the answers, with what.
pub struct CriticSeat {
    pub author: Author,
    pub model: Arc<dyn Model>,
    /// The critic's system text.
    pub system: Source,
    pub thinking: Option<Switch>,
    /// The request that follows the wrapped answer in the critic's last
    /// message. `None`: `Please evaluate the generator's response above.` A
    /// request that resolves to a blank text leaves the wrapped answer alone.
    pub review_request: Option<Source>,
    /// Whether the default verdict schema asks for `thinking` before
    /// `is_valid` and `message`, whatever `thinking` says. Default `true`.
    pub reasoning_field: bool,
    /// The description of `thinking` in the default schema. `None`:
    /// `Explanation of the thought process and validation steps`.
    pub thinking_description: Option<Source>,
    /// The description of `is_valid` in the default schema. `None`:
    /// `Boolean indicating if the generated message is valid`.
    pub is_valid_description: Option<Source>,
    /// The description of `message` in the default schema. `None`:
    /// `Feedback message for the generator, empty if validation passed`.
    pub message_description: Option<Source>,
    /// A verdict schema of the caller's, instead of the default one;
    /// `reasoning_field` and the descriptions are then ignored.
    pub verdict: Option<VerdictOverride>,
}

impl CriticSeat {
    /// A critic with the default request, the default verdict schema and no
    /// thinking setting.
    pub fn new(author: Author, model: Arc<dyn Model>, system: Source) -> Self {
        Self {
            author,
            model,
            system,
            thinking: None,
            review_request: None,
            reasoning_field: true,
            thinking_description: None,
            is_valid_description: None,
            message_description: None,
            verdict: None,
        }
    }
}

/// The request that follows the wrapped answer, by default.
const REVIEW_REQUEST: &str = "Please evaluate the generator's response above.";

/// The critique that rejects an empty answer, by default.
const EMPTY_ANSWER_CRITIQUE: &str = "The answer was empty. Write a complete answer.";

/// A generator and a critic writing in one shared history, as a graph.
///
/// The generator reads its system text, then `generator_history` from its own
/// perspective: its turns (tool traffic and thinking included) as assistant
/// messages, the critiques as framed messages. The critic reads its system
/// text, then `conversation` before the answer it judges, rendered as the
/// generator sees it with the generator's thinking removed (an answer left
/// with nothing is left out), then one user message holding
/// `<generator_response>\n{answer}\n</generator_response>`, a blank line
/// and the review request (the request is left out when blank). That message
/// stays separate from a user message before it.
///
/// Every text is a `Source`: each `State` key a source reads is a string
/// input, each `Config` key a string configuration key.
///
/// - Inputs: `conversation` (the shared history the critic reads; a new call
///   holds one user input), `generator_history` (the generator's working
///   history; when empty at entry it starts as a copy of `conversation`),
///   the state keys of the text sources, the generator's `enabled` key when
///   set, and each key of `inputs`, for the generator's tools to read through
///   the state they receive. A source, `enabled` or `inputs` key the graph
///   already has is refused (`KeyCollision`).
/// - Outputs: `answer` (string, the generator's last answer since the last
///   user input: the JSON text of its structured block when it has one,
///   else its text blocks joined verbatim; empty if none), `is_validated`,
///   `nb_generations` (the generator answers in `conversation` since its
///   last user input) and, when the generator's round limit has a flag, that
///   flag (true when the limit was reached in at least one generation of this
///   call). The histories are not outputs: a host that resumes a call gives
///   back the histories it saved from the observer. The run ends with
///   `validated` or `exhausted`.
/// - Configuration: the keys the text sources, switches and limits read.
///
/// `generator_history` holds the generator's whole traffic (tool calls and
/// results, answers) and the critiques; `conversation` holds the user input,
/// the answers (final step only) and the critiques. A critique is a turn of
/// the critic holding one text step, the verdict's `message`, appended to
/// both; nothing is appended on validation, and nothing of the critic's
/// reasoning enters a history.
///
/// The critic answers a structured verdict, by default `thinking`,
/// `is_valid` and `message` in that order, whatever its thinking switch (see
/// `CriticSeat`). A missing structured block, a block that does not read as
/// the default verdict (without a reader) or a rejection with an empty
/// `message` fails the run with `Structured`; an error of a `VerdictReader`
/// reaches the caller as the reader returned it.
///
/// With `prepare`, the graph enters through it, once, then the routing.
/// The entry is routed on how `conversation` ends: a generator answer goes
/// to the critic and a critique (a turn of the critic) is routed as a
/// rejection (both are resumed calls); anything else, another author's turn
/// or a turn without author included, goes to the generator. After an
/// answer, more than `max_critiques` answers end `exhausted` without the
/// critic; otherwise the critic judges it. A valid verdict ends `validated`;
/// a rejection sends the generator back, until `max_critiques` answers were
/// rejected: then the generator answers once more, unassessed, when
/// `final_generation`, else the run ends `exhausted` on the rejected answer.
/// More answers than that (a resumed call whose budget is now smaller) end
/// `exhausted` too; the same rule routes a critique found on entry.
/// A `max_critiques` of 0 gives one generation and no critic call: the run
/// ends `exhausted`, whatever `final_generation`.
///
/// An empty answer (its final step holds no non-blank text and no structured
/// block) is rejected without calling the critic: where the critic would
/// judge it, a critique of the critic holding `empty_answer_critique` is
/// appended to both histories, and the run goes on as after a rejection. The
/// empty answer counts as a generation.
///
/// With tools, the generation is the ReAct loop of `ReactAgent` inline in
/// this graph; each generation opens a new generator turn, so the round limit
/// counts per generation. A generation cut by `OnLimit::End` (its final step
/// is the limit's closing step) is an empty answer: rejected without the
/// critic where the critic would judge it, left out of what the critic reads
/// in later rounds (the user messages around it merged), and, as the last
/// generation, an empty `answer`. The limit's flag output is true when the round limit was
/// reached in at least one generation of this call.
pub struct GeneratorCritic {
    pub generator: GeneratorSeat,
    pub critic: CriticSeat,
    pub max_critiques: Count,
    pub final_generation: bool,
    /// Extra state keys the caller maps per run, read by the generator's
    /// tools.
    pub inputs: Vec<(Key, Kind)>,
    /// The critique that rejects an empty answer, written without calling
    /// the critic. `None`, or a source that resolves to a blank text: `The
    /// answer was empty. Write a complete answer.`
    pub empty_answer_critique: Option<Source>,
    /// A node run once, as the graph's entry, before the routing: it may do
    /// I/O (it holds its capabilities as fields) and writes through ordinary
    /// updates, typically the opening message into `conversation`. It is
    /// checked through `Node::check` like any node; its failure fails the
    /// graph. A resumed caller restarts the whole call, `prepare` included.
    pub prepare: Option<Box<dyn Node>>,
}

/// The names of the graph's keys and nodes.
struct Names {
    conversation: Key,
    generator_history: Key,
    answer: Key,
    validated: Key,
    generations: Key,
    prepare: NodeId,
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
            answer: Key::new("answer")?,
            validated: Key::new("is_validated")?,
            generations: Key::new("nb_generations")?,
            prepare: NodeId::new("prepare")?,
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
    critic: Author,
    max_critiques: Count,
    final_generation: bool,
    generate: NodeId,
    critique: NodeId,
    finish: NodeId,
}

impl Router {
    /// On entry: a generator answer last goes to the critic, a critique (a
    /// turn of the critic) last is routed as after a rejection, anything else
    /// (the opening input, another author's turn, a turn without author, an
    /// empty history) goes to the generator.
    fn entry(&self, state: &State, config: &Config) -> Result<Vec<Target>, GraphError> {
        match state.conversation(&self.conversation)?.entries().last() {
            Some(Entry::Turn(turn)) if turn.author() == Some(&self.generator) => {
                self.after_answer(state, config)
            }
            Some(Entry::Turn(turn)) if turn.author() == Some(&self.critic) => {
                self.after_rejection(state, config)
            }
            Some(Entry::Turn(_) | Entry::UserInput(_)) | None => {
                Ok(vec![Target::Node(self.generate.clone())])
            }
        }
    }

    fn after_answer(&self, state: &State, config: &Config) -> Result<Vec<Target>, GraphError> {
        let generations = self.generations(state)?;
        let max = self.max_critiques.resolve(config)?;
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
        self.after_rejection(state, config)
    }

    /// The generator answers again while fewer than `max_critiques` answers
    /// were given, once more (unassessed) at exactly `max_critiques` when
    /// `final_generation`; beyond that the run finishes, as when a resumed
    /// call reads a smaller budget.
    fn after_rejection(&self, state: &State, config: &Config) -> Result<Vec<Target>, GraphError> {
        let generations = self.generations(state)?;
        let max = self.max_critiques.resolve(config)?;
        let next = if generations < max || (self.final_generation && generations == max) {
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
    pub fn new(generator: GeneratorSeat, critic: CriticSeat, max_critiques: Count) -> Self {
        Self {
            generator,
            critic,
            max_critiques,
            final_generation: true,
            inputs: Vec::new(),
            empty_answer_critique: None,
            prepare: None,
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
            .state(names.answer.clone(), Kind::Str)
            .state(names.validated.clone(), Kind::Bool)
            .state(names.generations.clone(), Kind::Int);
        schema = declare_switch(schema, generator.thinking.as_ref());
        schema = declare_switch(schema, critic.thinking.as_ref());
        schema = declare_count(schema, &self.max_critiques);
        schema = declare_limit(schema, generator.tool_concurrency.as_ref());
        if let Some(round_limit) = &generator.round_limit {
            schema = declare_limit(schema, Some(&round_limit.max_rounds));
        }
        let mut schema = schema.build();
        let flag = generator.round_limit.as_ref().and_then(round_limit_flag);
        if let Some(flag) = &flag {
            schema.state.entry(flag.clone()).or_insert(Kind::Bool);
        }
        let review_request = critic
            .review_request
            .unwrap_or_else(|| Source::Const(REVIEW_REQUEST.to_owned()));
        let empty_answer_critique = self
            .empty_answer_critique
            .unwrap_or_else(|| Source::Const(EMPTY_ANSWER_CRITIQUE.to_owned()));
        let verdict = match critic.verdict {
            Some(custom) => VerdictForm::Override(custom),
            None => VerdictForm::default_form(
                critic.reasoning_field,
                critic.thinking_description,
                critic.is_valid_description,
                critic.message_description,
            ),
        };
        verdict.check()?;
        let mut sources = vec![
            &generator.system,
            &critic.system,
            &review_request,
            &empty_answer_critique,
        ];
        sources.extend(verdict.sources());
        let text_inputs = declare_sources(&mut schema, sources)?;
        if let Some(key) = &generator.enabled {
            declare_input(&mut schema, key, Kind::list(Kind::Str))?;
        }
        for (key, kind) in &self.inputs {
            declare_input(&mut schema, key, kind.clone())?;
        }
        self.max_critiques.check(&schema)?;

        let router = Router {
            conversation: names.conversation.clone(),
            validated: names.validated.clone(),
            generator: generator.author.clone(),
            critic: critic.author.clone(),
            max_critiques: self.max_critiques,
            final_generation: self.final_generation,
            generate: names.generate.clone(),
            critique: names.critique.clone(),
            finish: names.finish.clone(),
        };

        let mut builder = GraphBuilder::new(schema)
            .entry(match &self.prepare {
                Some(_) => names.prepare.clone(),
                None => names.route.clone(),
            })
            .input(names.conversation.clone())
            .input(names.generator_history.clone())
            .output(names.answer.clone())
            .output(names.validated.clone())
            .output(names.generations.clone());
        if let Some(flag) = flag {
            builder = builder.output(flag);
        }
        for key in text_inputs {
            builder = builder.input(key);
        }
        if let Some(key) = &generator.enabled {
            builder = builder.input(key.clone());
        }
        for (key, _) in self.inputs {
            builder = builder.input(key);
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
            system: vec![generator.system],
            tools: generator.tools,
            enabled: generator.enabled,
            output: generator.output,
            thinking: generator.thinking,
        };
        let mut builder = react.add(builder, llm)?;

        if let Some(prepare) = self.prepare {
            builder = builder.node(names.prepare.clone(), Prepare(prepare)).edge(
                names.prepare.clone(),
                Always(Target::Node(names.route.clone())),
            );
        }
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
                    generator: generator.author.clone(),
                    generator_history: names.generator_history.clone(),
                    validated: names.validated.clone(),
                    author: critic.author.clone(),
                    model: critic.model,
                    system: critic.system,
                    thinking: critic.thinking,
                    review_request,
                    empty_answer_critique,
                    verdict,
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
                    generations: names.generations,
                    generator: generator.author,
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

/// The caller's `prepare` node, run as it is.
struct Prepare(Box<dyn Node>);

impl Node for Prepare {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        self.0.run(state, config, ctx)
    }

    fn check(&self, schema: &Schema, site: CheckSite) -> Result<(), GraphError> {
        self.0.check(schema, site)
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

/// Asks the critic for a verdict on `conversation`, or rejects an empty
/// answer without asking. A rejection appends its message, as a turn of the
/// critic, to both histories.
struct CritiqueNode {
    conversation: Key,
    generator: Author,
    generator_history: Key,
    validated: Key,
    author: Author,
    model: Arc<dyn Model>,
    system: Source,
    thinking: Option<Switch>,
    review_request: Source,
    empty_answer_critique: Source,
    verdict: VerdictForm,
}

impl Node for CritiqueNode {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let conversation = state.conversation(&self.conversation)?;
            let verdict = match last_answer(conversation, &self.generator) {
                Some((index, step)) if !empty_step(step) => {
                    let messages = self.messages(state, config, conversation, index, step)?;
                    self.judge(state, config, ctx, messages).await?
                }
                Some(_) | None => Verdict {
                    is_valid: false,
                    message: self.empty_answer_text(state, config)?,
                },
            };
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

impl CritiqueNode {
    /// The critique of an empty answer: `empty_answer_critique`, or the
    /// default text when it resolves to a blank text.
    fn empty_answer_text(&self, state: &State, config: &Config) -> Result<String, GraphError> {
        let text = self.empty_answer_critique.resolve(state, config)?;
        if text.trim().is_empty() {
            Ok(EMPTY_ANSWER_CRITIQUE.to_owned())
        } else {
            Ok(text)
        }
    }

    /// What the critic reads: `conversation` before the answer at `index`,
    /// rendered as the generator sees it with the generator's thinking
    /// removed, then one user message holding the answer, wrapped, and the
    /// review request. A generation the round limit cut is an empty answer:
    /// it is left out, like an answer left with nothing once its thinking is
    /// removed, and the user messages around it are merged.
    fn messages(
        &self,
        state: &State,
        config: &Config,
        conversation: &Conversation,
        index: usize,
        answer: &Step,
    ) -> Result<Vec<WireMessage>, GraphError> {
        let mut earlier = Conversation::new();
        for entry in conversation.entries().get(..index).unwrap_or_default() {
            match entry {
                Entry::UserInput(input) => earlier.push_input(input.clone()),
                Entry::Turn(turn) if cut_generation(turn, &self.generator) => {}
                Entry::Turn(turn) => earlier.push_turn(turn.clone())?,
            }
        }
        let mut messages = if earlier.entries().is_empty() {
            Vec::new()
        } else {
            without_thinking(wire(&earlier, &self.generator)?)
        };
        let mut text = format!(
            "<generator_response>\n{}\n</generator_response>",
            answer_text(answer)
        );
        let request = self.review_request.resolve(state, config)?;
        if !request.trim().is_empty() {
            text.push_str("\n\n");
            text.push_str(&request);
        }
        messages.push(WireMessage::User {
            content: vec![WireUserBlock::Text {
                text: Text::new(text)?,
            }],
        });
        Ok(messages)
    }

    async fn judge(
        &self,
        state: &State,
        config: &Config,
        ctx: &Context,
        messages: Vec<WireMessage>,
    ) -> Result<Verdict, GraphError> {
        let thinking = match &self.thinking {
            Some(switch) => Some(switch.resolve(config)?),
            None => None,
        };
        let request = Request {
            system: Some(self.system.resolve(state, config)?),
            messages,
            tools: Vec::new(),
            tool_calls: ToolCalls::Allowed,
            output: OutputMode::Structured {
                schema: self.verdict.schema(state, config)?,
            },
            thinking,
        };
        let step = complete(self.model.as_ref(), request, ctx, &self.conversation).await?;
        self.verdict.read(&step)
    }
}

/// The generator's last answer since the last user input: its index in
/// `conversation` and its final step.
fn last_answer<'a>(
    conversation: &'a Conversation,
    generator: &Author,
) -> Option<(usize, &'a Step)> {
    let entries = conversation.entries();
    let start = entries.len() - since_last_input(conversation).len();
    entries
        .iter()
        .enumerate()
        .skip(start)
        .rev()
        .find_map(|(index, entry)| match entry {
            Entry::Turn(turn) if turn.author() == Some(generator) => {
                final_step(turn).map(|step| (index, step))
            }
            Entry::Turn(_) | Entry::UserInput(_) => None,
        })
}

/// Whether `turn` is a generation of `generator` that the round limit cut
/// (`OnLimit::End`'s closing step as its final step).
fn cut_generation(turn: &Turn, generator: &Author) -> bool {
    turn.author() == Some(generator) && final_step(turn).is_some_and(closes_on_limit)
}

/// Whether an answer is empty: no non-blank text and no structured block,
/// or a generation the round limit cut (`OnLimit::End`'s closing step).
fn empty_step(step: &Step) -> bool {
    closes_on_limit(step)
        || (step.structured().next().is_none()
            && step.text().all(|text| text.as_str().trim().is_empty()))
}

/// Removes the thinking blocks of every assistant message. A message left
/// with nothing is dropped, and the user messages it separated are merged,
/// as `render` merges adjacent user messages.
fn without_thinking(messages: Vec<WireMessage>) -> Vec<WireMessage> {
    let mut kept: Vec<WireMessage> = Vec::new();
    for message in messages {
        let message = match message {
            WireMessage::Assistant { content } => {
                let content: Vec<AssistantBlock> = content
                    .into_iter()
                    .filter(|block| {
                        !matches!(
                            block,
                            AssistantBlock::Thinking(_) | AssistantBlock::RedactedThinking(_)
                        )
                    })
                    .collect();
                if content.is_empty() {
                    continue;
                }
                WireMessage::Assistant { content }
            }
            other @ (WireMessage::User { .. } | WireMessage::Relay { .. }) => other,
        };
        if let WireMessage::User { content: more } = &message
            && let Some(WireMessage::User { content }) = kept.last_mut()
        {
            content.extend(more.iter().cloned());
            continue;
        }
        kept.push(message);
    }
    kept
}

/// Sets `answer` and `nb_generations` from `conversation`.
struct FinishNode {
    conversation: Key,
    answer: Key,
    generations: Key,
    generator: Author,
}

impl Node for FinishNode {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let conversation = state.conversation(&self.conversation)?;
            let generations = since_last_input(conversation)
                .iter()
                .filter(|entry| authored_by(entry, &self.generator))
                .count();
            let answer = match last_answer(conversation, &self.generator) {
                Some((_, step)) if !closes_on_limit(step) => answer_text(step),
                Some(_) | None => String::new(),
            };
            Ok(vec![
                Update::Set {
                    key: self.answer.clone(),
                    value: Value::str(answer),
                },
                Update::Set {
                    key: self.generations.clone(),
                    value: Value::int(i64::try_from(generations).unwrap_or(i64::MAX)),
                },
            ])
        })
    }
}
