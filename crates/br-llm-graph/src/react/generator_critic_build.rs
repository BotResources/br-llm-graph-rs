//! The build of the generator-critic graph: its keys, its nodes and its
//! edges.

use std::sync::Arc;

use crate::error::GraphError;
use crate::graph::{Always, FnEdge, Graph, GraphBuilder, Target};
use crate::react::agent_build::{
    AgentKeys, declare_count, declare_limit, round_limit_flag, split_tools, tools_node_clash,
};
use crate::react::bounds::Bounds;
use crate::react::check::{CheckNode, EMPTY_ANSWER_CRITIQUE};
use crate::react::critic_view::CriticView;
use crate::react::critique::{CritiqueNode, REVIEW_REQUEST};
use crate::react::declarations::Declarations;
use crate::react::duo::Duo;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_names::Names;
use crate::react::generator_critic_nodes::{FinishNode, Prepare, RouteNode, finish_edge};
use crate::react::generator_critic_routing::Router;
use crate::react::given::{History, Prompt, Reads};
use crate::react::llm_node::LlmNode;
use crate::react::react_loop::ReactLoop;
use crate::react::round_limit::OnLimit;
use crate::react::seats::CriticHistory;
use crate::react::verdict::VerdictForm;
use crate::state::{Kind, Schema};

impl GeneratorCritic {
    /// The graph of this generator-critic, or the first refusal of its
    /// build (see the type's documentation).
    pub fn graph(self) -> Result<Arc<Graph>, GraphError> {
        if self.generator.author == self.critic.author {
            return Err(GraphError::SameAuthor {
                author: self.generator.author,
            });
        }
        let keys = AgentKeys::with_turns(self.thread_store.is_some())?;
        let names = Names::new()?;
        let generator = self.generator;
        let critic = self.critic;

        let flag = generator.round_limit.as_ref().and_then(round_limit_flag);
        if let Some(flag) = &flag
            && [
                &names.validated,
                &keys.opening,
                &keys.thread_name,
                &keys.conversation,
            ]
            .contains(&flag)
        {
            return Err(GraphError::KeyCollision { key: flag.clone() });
        }
        let mut schema = Schema::builder()
            .state_by_name(keys.opening.clone(), Kind::Conversation)
            .state_by_name(keys.answer.clone(), Kind::Str)
            .state_by_name(names.validated.clone(), Kind::Bool)
            .state_by_name(names.generations.clone(), Kind::Int);
        if let Some(thread) = &keys.thread {
            schema = schema.state_by_name(thread.clone(), Kind::Str);
        }
        for count in [
            &self.max_critiques,
            &self.revision_limit,
            &self.verdict_retries,
        ] {
            schema = declare_count(schema, count);
        }
        schema = declare_limit(schema, generator.tool_concurrency.as_ref());
        if let Some(round_limit) = &generator.round_limit {
            schema = declare_limit(schema, Some(&round_limit.max_rounds));
        }
        let mut schema = schema.build();
        keys.reserve(&mut schema);
        if let Some(flag) = &flag {
            schema.state.entry(flag.clone()).or_insert(Kind::Bool);
        }
        let mut reserved = keys.reserved(flag.as_ref());
        reserved.extend([names.validated.clone(), names.generations.clone()]);

        let review_request = critic
            .review_request
            .unwrap_or_else(|| Prompt::text(REVIEW_REQUEST));
        let empty_answer_critique = self
            .empty_answer_critique
            .unwrap_or_else(|| Prompt::text(EMPTY_ANSWER_CRITIQUE));
        let verdict = VerdictForm::new(
            critic.verdict,
            critic.thinking_description,
            critic.is_valid_description,
            critic.message_description,
        );
        verdict.check()?;
        let critic_history_reads = match &critic.history {
            CriticHistory::Brief(key) => Some(Reads::of_state(key)),
            CriticHistory::Given(history) => Some(history.reads().clone()),
            CriticHistory::History | CriticHistory::Latest => None,
        };
        let mut declarations = Declarations::new(schema, reserved);
        let reads = [
            generator.model.reads(),
            generator.system.reads(),
            critic.model.reads(),
            critic.system.reads(),
            review_request.reads(),
            empty_answer_critique.reads(),
        ]
        .into_iter()
        .chain(generator.history.as_ref().map(History::reads))
        .chain(critic_history_reads.as_ref())
        .chain(verdict.reads())
        .chain(self.rule_check.as_ref().map(|check| check.reads()));
        for reads in reads {
            declarations.reads(reads)?;
        }
        if let Some(key) = &generator.enabled {
            declarations.input(key, Kind::list(Kind::Str))?;
        }
        for (key, kind) in self.inputs {
            declarations.input(&key, kind)?;
        }
        let (mut schema, declared) = declarations.finish();
        keys.release(&mut schema);
        for count in [
            &self.max_critiques,
            &self.revision_limit,
            &self.verdict_retries,
        ] {
            count.check(&schema)?;
        }

        let call = keys.call(self.thread_store);
        let limit_ends = matches!(
            generator.round_limit.as_ref().map(|limit| &limit.on_limit),
            Some(OnLimit::End { .. })
        );
        let duo = Duo {
            call: call.clone(),
            conversation: keys.conversation.clone(),
            turns: keys.turns.clone(),
            generator: generator.author.clone(),
            critic: critic.author.clone(),
            budget: self.max_critiques,
            final_generation: self.final_generation,
            revision_limit: self.revision_limit,
            limit_ends,
        };
        let bounds = Bounds::new(
            [&duo.budget, &duo.revision_limit, &self.verdict_retries],
            [
                generator.tool_concurrency.as_ref(),
                generator
                    .round_limit
                    .as_ref()
                    .map(|limit| &limit.max_rounds),
            ],
        );
        let router = Router {
            duo: duo.clone(),
            bounds,
            generate: names.generate.clone(),
            check: names.check.clone(),
            critique: names.critique.clone(),
            finish: names.finish.clone(),
        };

        let builder = keys
            .declare_private(GraphBuilder::new(schema))
            .entry(match &self.prepare {
                Some(_) => names.prepare.clone(),
                None => names.route.clone(),
            });
        let builder = keys.signature(
            builder,
            declared,
            &[&names.validated, &names.generations],
            flag.as_ref(),
        );

        let clash = tools_node_clash(
            &generator.tool_nodes,
            generator.round_limit.as_ref(),
            &names.tools,
        );
        let tool_nodes = split_tools(generator.tool_nodes, &generator.tools, &names.tools);
        let flag_limit = flag.zip(generator.round_limit.clone());
        let react = ReactLoop {
            llm: names.generate.clone(),
            tool_nodes,
            after: Target::Node(names.check.clone()),
            tool_concurrency: generator.tool_concurrency,
            round_limit: generator.round_limit,
            thread: call.thread.clone(),
        };
        let llm = LlmNode {
            key: keys.conversation.clone(),
            author: generator.author,
            model: generator.model,
            system: generator.system,
            history: generator.history,
            tools: generator.tools,
            enabled: generator.enabled,
            output_mode: generator.output_mode,
            call,
            seat: Some(duo.clone()),
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

        let (entry, checked, judged) = (router.clone(), router.clone(), router);
        let graph = builder
            .node(names.route.clone(), RouteNode { duo: duo.clone() })
            .edge(
                names.route,
                FnEdge::new(move |state, config| entry.entry(state, config)),
            )
            .node(
                names.check.clone(),
                CheckNode {
                    duo: duo.clone(),
                    rule_check: self.rule_check,
                    empty_answer_critique,
                },
            )
            .edge(
                names.check,
                FnEdge::new(move |state, config| checked.after_check(state, config)),
            )
            .node(
                names.critique.clone(),
                CritiqueNode {
                    model: critic.model,
                    system: critic.system,
                    view: CriticView {
                        history: critic.history,
                        review_request,
                        duo: duo.clone(),
                    },
                    verdict,
                    retries: self.verdict_retries,
                    stream_key: names.critic_stream,
                },
            )
            .edge(
                names.critique,
                FnEdge::new(move |state, config| judged.after_critique(state, config)),
            )
            .node(
                names.finish.clone(),
                FinishNode {
                    duo,
                    answer: keys.answer,
                    validated: names.validated.clone(),
                    generations: names.generations,
                    flag: flag_limit,
                },
            )
            .edge(names.finish, finish_edge(names.validated))
            .build()?;
        Ok(Arc::new(graph))
    }
}
