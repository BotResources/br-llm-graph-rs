//! The arguments of a runner call node: a JSON object fixed when the node is
//! built, or given by a function of the state and the configuration.

use std::sync::Arc;

use serde_json::{Map, Value};

use crate::graph::NodeError;
use crate::react::Origin;
use crate::state::{Config, State};

/// A function giving the arguments of one call.
type ArgumentsFn = dyn Fn(&State, &Config) -> Result<Map<String, Value>, NodeError> + Send + Sync;

/// What a runner call node sends its child, a JSON object: fixed when the
/// node is built (`RunnerArguments::value`), or given at each call by a
/// function of the state and the configuration (`RunnerArguments::from_fn`).
///
/// A function is synchronous and does no I/O: what it needs for every run is
/// given to it when the node is built, as what the closure captures. Its
/// error fails the node, as returned. A description says how the arguments
/// are given (`fixed`, `function`), never their value.
///
/// ```
/// use br_llm_graph::{RunnerArguments, StateKey};
/// use serde_json::{Map, Value};
///
/// let fixed = RunnerArguments::value(Map::from_iter([("lang".to_owned(), Value::from("fr"))]));
/// let question: StateKey<String> = br_llm_graph::Schema::builder().state("question").unwrap();
/// let asked = RunnerArguments::from_fn(move |state, _config| {
///     Ok(Map::from_iter([("question".to_owned(), Value::from(state.get(&question)?))]))
/// });
/// # let _ = (fixed, asked);
/// ```
#[derive(Clone)]
pub struct RunnerArguments {
    give: Arc<ArgumentsFn>,
    /// How they are given, for the graph's description.
    origin: Origin,
}

impl RunnerArguments {
    /// The fixed object `arguments`, sent on every call.
    pub fn value(arguments: Map<String, Value>) -> Self {
        Self {
            give: Arc::new(move |_, _| Ok(arguments.clone())),
            origin: Origin::Fixed,
        }
    }

    /// The function `give`, asked at each call.
    pub fn from_fn<F>(give: F) -> Self
    where
        F: Fn(&State, &Config) -> Result<Map<String, Value>, NodeError> + Send + Sync + 'static,
    {
        Self {
            give: Arc::new(give),
            origin: Origin::Function,
        }
    }

    /// The arguments of one call, on `state` and `config`.
    pub(crate) fn give(
        &self,
        state: &State,
        config: &Config,
    ) -> Result<Map<String, Value>, NodeError> {
        (self.give)(state, config)
    }

    /// How they are given: fixed or by a function.
    pub(crate) fn origin(&self) -> Origin {
        self.origin
    }
}
