//! The routing of the generator-critic graph: where the run goes on entry,
//! after an answer and after a critique, and the reads of `conversation` it
//! rests on.

use br_llm_messages::{Author, Conversation, Entry, Step};

use crate::error::GraphError;
use crate::graph::{Count, Target};
use crate::react::helpers::final_step;
use crate::state::{Config, State};
use crate::value::{Key, NodeId};

/// Where the run goes once the critic has seen an answer, or before.
#[derive(Clone)]
pub(crate) struct Router {
    pub(crate) conversation: Key,
    pub(crate) validated: Key,
    pub(crate) generator: Author,
    pub(crate) critic: Author,
    pub(crate) max_critiques: Count,
    pub(crate) final_generation: bool,
    pub(crate) generate: NodeId,
    pub(crate) critique: NodeId,
    pub(crate) finish: NodeId,
}

impl Router {
    /// On entry: a generator answer last goes to the critic, a critique (a
    /// turn of the critic) last is routed as after a rejection, anything else
    /// (the opening input, another author's turn, a turn without author, an
    /// empty history) goes to the generator.
    pub(crate) fn entry(&self, state: &State, config: &Config) -> Result<Vec<Target>, GraphError> {
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

    pub(crate) fn after_answer(
        &self,
        state: &State,
        config: &Config,
    ) -> Result<Vec<Target>, GraphError> {
        let generations = self.generations(state)?;
        let max = self.max_critiques.resolve(config)?;
        let next = if generations > max {
            &self.finish
        } else {
            &self.critique
        };
        Ok(vec![Target::Node(next.clone())])
    }

    pub(crate) fn after_critique(
        &self,
        state: &State,
        config: &Config,
    ) -> Result<Vec<Target>, GraphError> {
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

pub(crate) fn since_last_input(conversation: &Conversation) -> &[Entry] {
    let entries = conversation.entries();
    let start = entries
        .iter()
        .rposition(|entry| matches!(entry, Entry::UserInput(_)))
        .map_or(0, |index| index + 1);
    entries.get(start..).unwrap_or_default()
}

pub(crate) fn authored_by(entry: &Entry, author: &Author) -> bool {
    match entry {
        Entry::Turn(turn) => turn.author() == Some(author),
        Entry::UserInput(_) => false,
    }
}

/// The generator's last answer since the last user input: its index in
/// `conversation` and its final step.
pub(crate) fn last_answer<'a>(
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
