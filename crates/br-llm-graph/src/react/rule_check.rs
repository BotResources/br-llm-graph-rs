//! The rule check of a generator-critic: a judgment of each generation made
//! by code, before the critic.

use br_llm_messages::Text;

use crate::react::generator_critic::GeneratorCritic;
use crate::react::given::Given;

/// What the rule check gives for a generation: a pass, or a revision with
/// the text the generator reads. A failure is the function's error, which
/// fails the run as returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ruling {
    /// The generation goes on to the critic, or finishes when the critiques
    /// have reached the budget.
    Pass,
    /// The generator answers again, after a message of the critic holding
    /// this text (stop reason `other(rule_check)`). A generation has at most
    /// the revision limit of revisions in a row; after the last, the answer
    /// goes on as if it had passed.
    Revise(Text),
}

/// The rule check, a function of the generator-critic's context: the state,
/// the configuration, the conversation, the generation number, the budget
/// and the final step of the generation judged (`RequestContext::answer`:
/// its text, its structured block, its stop reason). It does not read an
/// empty answer, which is rejected before it.
pub type RuleCheck = Given<GeneratorCritic, Ruling>;
