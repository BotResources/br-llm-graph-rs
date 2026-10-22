//! The target of a runner call: the runner type of the runner it calls.

use crate::error::GraphError;

/// The runner type a runner call calls: a text that is not blank, kept as
/// given. A blank one (empty, or only whitespace) is refused
/// (`RunnerTargetBlank`).
///
/// ```
/// use br_llm_graph::{GraphError, RunnerTarget};
///
/// assert_eq!(RunnerTarget::new("documentalist").unwrap().as_str(), "documentalist");
/// assert!(matches!(RunnerTarget::new(" \n"), Err(GraphError::RunnerTargetBlank)));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RunnerTarget(String);

impl RunnerTarget {
    /// The target `runner_type`, or `RunnerTargetBlank` when it is blank.
    pub fn new(runner_type: impl Into<String>) -> Result<Self, GraphError> {
        let runner_type = runner_type.into();
        if runner_type.trim().is_empty() {
            return Err(GraphError::RunnerTargetBlank);
        }
        Ok(Self(runner_type))
    }

    /// Its text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RunnerTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
