//! The routing of the generator-critic graph: where the run goes on entry,
//! after the check and after the critic, read from the turns of the call.

use crate::error::GraphError;
use crate::graph::Target;
use crate::react::duo::{CallTurns, Duo, TurnKind};
use crate::state::{Config, State};
use crate::value::NodeId;

/// Where the run goes, read from the turns of the call.
#[derive(Clone)]
pub(crate) struct Router {
    pub(crate) duo: Duo,
    pub(crate) generate: NodeId,
    pub(crate) check: NodeId,
    pub(crate) critique: NodeId,
    pub(crate) finish: NodeId,
}

impl Router {
    /// On entry, on the last turn of the call: a generation goes to the
    /// check; a critique is routed as after a rejection; a revision as the
    /// check routes it; an acceptance finishes, without calling a model;
    /// none goes to the generator.
    pub(crate) fn entry(&self, state: &State, config: &Config) -> Result<Vec<Target>, GraphError> {
        let turns = self.duo.turns(state)?;
        let next = match turns.last().map(|(_, kind)| kind) {
            None => &self.generate,
            Some(TurnKind::Generation) => &self.check,
            Some(TurnKind::Critique) => self.after_rejection(&turns, config)?,
            Some(TurnKind::Revision) => self.revision_found(&turns, config)?,
            Some(TurnKind::Acceptance) => &self.finish,
        };
        Ok(vec![Target::Node(next.clone())])
    }

    /// After the check, on what it recorded: a revision sends the generator
    /// back; a critique of an empty answer is routed as a rejection; a
    /// generation the check let through goes to the critic, unless the
    /// critiques have reached the budget: then the run finishes.
    pub(crate) fn after_check(
        &self,
        state: &State,
        config: &Config,
    ) -> Result<Vec<Target>, GraphError> {
        let turns = self.duo.turns(state)?;
        let next = match turns.last().map(|(_, kind)| kind) {
            Some(TurnKind::Revision) => &self.generate,
            Some(TurnKind::Critique) => self.after_rejection(&turns, config)?,
            Some(TurnKind::Generation) => self.after_pass(&turns, config)?,
            Some(TurnKind::Acceptance) | None => &self.finish,
        };
        Ok(vec![Target::Node(next.clone())])
    }

    /// After the critic: an acceptance finishes, a critique is routed as a
    /// rejection.
    pub(crate) fn after_critique(
        &self,
        state: &State,
        config: &Config,
    ) -> Result<Vec<Target>, GraphError> {
        let turns = self.duo.turns(state)?;
        let next = match turns.last().map(|(_, kind)| kind) {
            Some(TurnKind::Critique) => self.after_rejection(&turns, config)?,
            Some(TurnKind::Acceptance | TurnKind::Generation | TurnKind::Revision) | None => {
                &self.finish
            }
        };
        Ok(vec![Target::Node(next.clone())])
    }

    /// A generation that passed the check: to the critic while the critiques
    /// are below the budget, else the run finishes without it.
    fn after_pass(&self, turns: &CallTurns<'_>, config: &Config) -> Result<&NodeId, GraphError> {
        let budget = self.duo.budget.resolve(config)?;
        Ok(if turns.critiques() < budget {
            &self.critique
        } else {
            &self.finish
        })
    }

    /// After a critique: the generator answers again while the critiques are
    /// below the budget, once more, unassessed, when `final_generation` and
    /// the critiques equal the budget; otherwise the run finishes
    /// `exhausted` on the rejected answer.
    fn after_rejection(
        &self,
        turns: &CallTurns<'_>,
        config: &Config,
    ) -> Result<&NodeId, GraphError> {
        let critiques = turns.critiques();
        let budget = self.duo.budget.resolve(config)?;
        Ok(
            if critiques < budget || (self.duo.final_generation && critiques == budget) {
                &self.generate
            } else {
                &self.finish
            },
        )
    }

    /// A revision found on entry, routed as the check routed it: the check
    /// records a revision only while the revisions in a row before it are
    /// below the revision limit, and the generator then answers again,
    /// whatever the budget. A revision past the limit read now (a limit read
    /// lower than when it was recorded) is not one the check would record:
    /// the generation before it goes on as if it had passed.
    fn revision_found(
        &self,
        turns: &CallTurns<'_>,
        config: &Config,
    ) -> Result<&NodeId, GraphError> {
        let before = turns.revisions_in_a_row().saturating_sub(1);
        if before < self.duo.revision_limit.resolve(config)? {
            return Ok(&self.generate);
        }
        self.after_pass(turns, config)
    }
}
