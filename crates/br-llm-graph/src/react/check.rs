//! The check of the generator-critic: each generation judged before the
//! critic, an empty answer rejected without it, the rule check asking for a
//! revision within the revision limit.

use br_llm_messages::Text;

use crate::graph::{Context, Node, NodeError, NodeFuture};
use crate::react::critic_writer::record;
use crate::react::duo::{Duo, TurnKind};
use crate::react::generator_critic::GeneratorCritic;
use crate::react::given::{CheckFacts, Prompt};
use crate::react::helpers::final_step;
use crate::react::rule_check::{RuleCheck, Ruling};
use crate::state::{Config, State};

/// The critique that rejects an empty answer, by default.
pub(crate) const EMPTY_ANSWER_CRITIQUE: &str = "The answer was empty. Write a complete answer.";

/// Judges the last generation of the call before the critic:
/// - an empty answer is rejected without the critic: where the critic would
///   judge it (the critiques below the budget), a critique holding the
///   empty-answer critique is recorded; else nothing is;
/// - otherwise the rule check, when set, passes it, asks for a revision,
///   recorded while the revisions in a row are below the revision limit, or
///   fails the node with its error, as returned.
///
/// What follows is the edge's (see `Router::after_check`): it reads what the
/// check recorded.
pub(crate) struct CheckNode {
    pub(crate) duo: Duo,
    pub(crate) rule_check: Option<RuleCheck>,
    pub(crate) empty_answer_critique: Prompt<GeneratorCritic>,
}

impl Node for CheckNode {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let duo = &self.duo;
            let turns = duo.turns(state)?;
            let Some(judged) = turns.last_generation() else {
                return Ok(Vec::new());
            };
            let Some(answer) = final_step(judged) else {
                return Ok(Vec::new());
            };
            let count = duo.count(config, &turns, turns.generations())?;
            let facts = CheckFacts {
                conversation: state.conversation(&duo.conversation)?,
                count,
                answer,
            };
            if duo.empty(judged) {
                if count.critiques >= count.budget {
                    return Ok(Vec::new());
                }
                let text = self.empty_answer_text(state, config, facts)?;
                return Ok(record(duo, state, ctx, TurnKind::Critique, text).await?);
            }
            let Some(rule_check) = &self.rule_check else {
                return Ok(Vec::new());
            };
            match rule_check.give(state, config, facts)? {
                Ruling::Revise(text)
                    if turns.revisions_in_a_row() < duo.revision_limit.resolve(config)? =>
                {
                    Ok(record(duo, state, ctx, TurnKind::Revision, text).await?)
                }
                Ruling::Revise(_) | Ruling::Pass => Ok(Vec::new()),
            }
        })
    }

    /// `node`, holding what the check reads.
    fn describe(&self) -> crate::describe::NodeDescription {
        self.description()
    }
}

impl CheckNode {
    /// The critique of an empty answer: `empty_answer_critique`, or the
    /// default text when it resolves to a blank text.
    fn empty_answer_text(
        &self,
        state: &State,
        config: &Config,
        facts: CheckFacts<'_>,
    ) -> Result<Text, NodeError> {
        let text = self.empty_answer_critique.give(state, config, facts)?;
        Ok(Text::new(text).or_else(|_| Text::new(EMPTY_ANSWER_CRITIQUE))?)
    }
}
