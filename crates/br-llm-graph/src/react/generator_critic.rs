use std::sync::Arc;

use br_llm_clients_core::ReasoningLevel;

use crate::error::GraphError;
use crate::graph::{Always, Count, FnEdge, Graph, GraphBuilder, Node, Target};
use crate::react::agent_build::{declare_count, declare_limit, round_limit_flag, tools_node_clash};
use crate::react::critique::{CritiqueNode, EMPTY_ANSWER_CRITIQUE, REVIEW_REQUEST};
use crate::react::declarations::Declarations;
use crate::react::generator_critic_nodes::{FinishNode, Prepare, PublishNode, RouteNode};
use crate::react::generator_critic_routing::Router;
use crate::react::given::{Model, Prompt};
use crate::react::llm_node::LlmNode;
use crate::react::react_loop::ReactLoop;
use crate::react::round_limit::OnLimit;
use crate::react::seats::{CriticSeat, GeneratorSeat};
use crate::react::verdict::VerdictForm;
use crate::state::{Kind, Schema};
use crate::value::{EndLabel, Key, NodeId};

/// A generator and a critic writing in one shared history, as a graph.
///
/// The generator reads its system text, then `generator_history` from its own
/// perspective: its turns (tool traffic and thinking included) as assistant
/// messages, the critiques as framed messages. The critic reads its system
/// text (always sent: a text that resolves to a blank one fails the node),
/// then `conversation` before the answer it judges, rendered as the
/// generator sees it with the generator's thinking removed (an answer left
/// with nothing is left out), then one user message holding
/// `<generator_response>\n{answer}\n</generator_response>`, a blank line
/// and the review request (the request is left out when blank). That message
/// stays separate from a user message before it.
///
/// Every text is a `Prompt`, given like a system (see `Given`): each state
/// key its function declares is an input of its kind, each configuration key
/// a configuration key of its kind. A key read by several functions, with
/// one kind, is declared once.
///
/// - Inputs: `conversation` (the shared history the critic reads; a new call
///   holds one user input), `generator_history` (the generator's working
///   history; when empty at entry it starts as a copy of `conversation`),
///   the state keys the texts' functions declare, the generator's `enabled`
///   key when set, and each key of `inputs`, for the generator's tools to
///   read through the state they receive. A key the functions declare,
///   `enabled` or an `inputs` key that the graph already has is refused
///   (`KeyCollision`).
/// - Outputs: `answer` (string, the generator's last answer since the last
///   user input: the JSON text of its structured block when it has one,
///   else its text blocks joined verbatim; empty if none), `is_validated`,
///   `nb_generations` (the generator answers in `conversation` since its
///   last user input) and, when the generator's round limit has a flag, that
///   flag (true when the limit was reached in at least one generation of this
///   call). The histories are not outputs: a host that resumes a call gives
///   back the histories it saved from the observer. The run ends with
///   `validated` or `exhausted`.
/// - Configuration: the keys the texts' functions declare, the keys the
///   budget and the limits read.
///
/// `generator_history` holds the generator's whole traffic (tool calls and
/// results, answers) and the critiques; `conversation` holds the user input,
/// the answers (final step only) and the critiques. A critique is a turn of
/// the critic holding one text step, the verdict's `message`, appended to
/// both; nothing is appended on validation, and nothing of the critic's
/// reasoning enters a history.
///
/// The critic answers a structured verdict, by default the shape `verdict`:
/// `thinking` (only when the `ReasoningLevel` of the critic's client is
/// `Off`), `is_valid` and `message`, in that order, all required (see
/// `CriticSeat`). A missing structured block, a block that does not read as
/// the default verdict (without a reader) or a rejection with a blank
/// `message` (empty or whitespace only) fails the run with `Structured`; an
/// error of a `VerdictReader` reaches the caller as the reader returned it.
/// A conforming client refuses a reply without a structured block itself
/// (`MalformedResponse`): the critique then fails with `Model`.
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
/// empty answer counts as a generation. A conforming client refuses a reply
/// that holds nothing but thinking (`ReplyUnusable`): the generation then
/// fails with `Model`.
///
/// With tools, the generation is the ReAct loop of `ReactAgent` inline in
/// this graph; each generation opens a new generator turn, so the round limit
/// counts per generation. A generation cut by `OnLimit::End` (its final step
/// is the closing step the limit node writes: the stop reason
/// `other(tool_round_limit)`, no model, no usage; a provider's reply keeping
/// that stop reason carries a model and is an answer) is an empty answer: rejected without the
/// critic where the critic would judge it, left out of what the critic reads
/// in later rounds (the user messages around it merged), and, as the last
/// generation, an empty `answer`. The limit's flag output is true when the
/// round limit was reached in at least one generation of this call.
pub struct GeneratorCritic {
    pub generator: GeneratorSeat,
    pub critic: CriticSeat,
    pub max_critiques: Count,
    pub final_generation: bool,
    /// Extra state keys the caller maps per run, read by the generator's
    /// tools.
    pub inputs: Vec<(Key, Kind)>,
    /// The critique that rejects an empty answer, written without calling
    /// the critic. `None`, or a text that resolves blank: `The
    /// answer was empty. Write a complete answer.`
    pub empty_answer_critique: Option<Prompt<CriticSeat>>,
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
            .state_by_name(names.conversation.clone(), Kind::Conversation)
            .state_by_name(names.generator_history.clone(), Kind::Conversation)
            .state_by_name(names.answer.clone(), Kind::Str)
            .state_by_name(names.validated.clone(), Kind::Bool)
            .state_by_name(names.generations.clone(), Kind::Int);
        schema = declare_count(schema, &self.max_critiques);
        schema = declare_limit(schema, generator.tool_concurrency.as_ref());
        if let Some(round_limit) = &generator.round_limit {
            schema = declare_limit(schema, Some(&round_limit.max_rounds));
        }
        let mut schema = schema.build();
        let flag = generator.round_limit.as_ref().and_then(round_limit_flag);
        let limit_ends = matches!(
            generator.round_limit.as_ref().map(|limit| &limit.on_limit),
            Some(OnLimit::End { .. })
        );
        if let Some(flag) = &flag {
            if schema.state.get(flag) == Some(&Kind::Bool) {
                return Err(GraphError::KeyCollision { key: flag.clone() });
            }
            schema.state.entry(flag.clone()).or_insert(Kind::Bool);
        }
        let review_request = critic
            .review_request
            .unwrap_or_else(|| Prompt::text(REVIEW_REQUEST));
        let empty_answer_critique = self
            .empty_answer_critique
            .unwrap_or_else(|| Prompt::text(EMPTY_ANSWER_CRITIQUE));
        let verdict = match critic.verdict {
            Some(custom) => VerdictForm::Override(custom),
            None => VerdictForm::default_form(
                critic.client.settings().reasoning() == ReasoningLevel::Off,
                critic.thinking_description,
                critic.is_valid_description,
                critic.message_description,
            ),
        };
        verdict.check()?;
        let mut reads = vec![
            generator.system.reads(),
            critic.system.reads(),
            review_request.reads(),
            empty_answer_critique.reads(),
        ];
        reads.extend(verdict.reads());
        let reserved = [
            &names.conversation,
            &names.generator_history,
            &names.answer,
            &names.validated,
            &names.generations,
        ]
        .into_iter()
        .chain(flag.as_ref())
        .cloned()
        .collect();
        let mut declarations = Declarations::new(schema, reserved);
        for reads in reads {
            declarations.reads(reads)?;
        }
        if let Some(key) = &generator.enabled {
            declarations.input(key, Kind::list(Kind::Str))?;
        }
        for (key, kind) in &self.inputs {
            declarations.input(key, kind.clone())?;
        }
        let (schema, inputs) = declarations.finish();
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
        for key in inputs {
            builder = builder.input(key);
        }

        let tools_node = NodeId::new("tools")?;
        let clash = tools_node_clash(
            &generator.tool_nodes,
            generator.round_limit.as_ref(),
            &tools_node,
        );
        let tool_nodes = if generator.tool_nodes.is_empty() && !generator.tools.is_empty() {
            vec![(tools_node, generator.tools.clone())]
        } else {
            generator.tool_nodes
        };
        let react = ReactLoop {
            llm: names.generate.clone(),
            tool_nodes,
            after: Target::Node(names.publish.clone()),
            tool_concurrency: generator.tool_concurrency,
            round_limit: generator.round_limit,
            thread: None,
        };
        let llm = LlmNode {
            key: names.generator_history.clone(),
            author: generator.author.clone(),
            model: Model::client(generator.client),
            system: generator.system,
            history: None,
            tools: generator.tools,
            enabled: generator.enabled,
            output_mode: generator.output_mode,
            call: None,
        };
        let mut builder = react.add(builder, llm)?;
        if let Some(clash) = clash {
            return Err(clash);
        }

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
                    client: critic.client,
                    system: critic.system,
                    review_request,
                    empty_answer_critique,
                    verdict,
                    limit_ends,
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
                    limit_ends,
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
