//! The counts and limits an agent reads from its configuration, refused on
//! entry.

use crate::error::GraphError;
use crate::graph::{Count, Limit};
use crate::state::Config;

/// The counts and limits an agent reads from its configuration: the
/// generator-critic's budget, revision limit and verdict retries, the reply
/// retries of the model call, and the tool concurrency and round limit of
/// the ReAct loop.
///
/// The edge of the agent's entry node checks them all, in that order, so a
/// refused one (`CountNegative`, `LimitNotPositive`, or the `UnknownKey` or
/// `KindMismatch` of its key) is the run's error itself, as an edge refusal
/// is, before any node reads it and before any model is called. The nodes
/// that read them afterwards read the configuration this check accepted.
#[derive(Clone, Default)]
pub(crate) struct Bounds {
    counts: Vec<Count>,
    limits: Vec<Limit>,
}

impl Bounds {
    /// The bounds of an agent, its counts then its limits; a limit given as
    /// `None` is not set.
    pub(crate) fn new<'a>(
        counts: impl IntoIterator<Item = &'a Count>,
        limits: impl IntoIterator<Item = Option<&'a Limit>>,
    ) -> Self {
        Self {
            counts: counts.into_iter().cloned().collect(),
            limits: limits.into_iter().flatten().cloned().collect(),
        }
    }

    /// Resolves each bound from `config`, refusing the first that does not
    /// resolve.
    pub(crate) fn check(&self, config: &Config) -> Result<(), GraphError> {
        for count in &self.counts {
            count.resolve(config)?;
        }
        for limit in &self.limits {
            limit.resolve(config)?;
        }
        Ok(())
    }
}
