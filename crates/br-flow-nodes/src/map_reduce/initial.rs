use std::num::NonZeroUsize;
use std::sync::Arc;

use br_graph_contract::{Fields, State};

use super::{MapReduceError, Rules};

pub(super) fn states<F: Fields, G: Fields>(
    rules: &Rules<F, G>,
    internal_size_limit: NonZeroUsize,
    state: &State<F>,
) -> Result<Vec<Arc<State<G>>>, MapReduceError> {
    (rules.map)(state)
        .into_iter()
        .enumerate()
        .map(|(position, fields)| {
            State::create_with_limit(fields, internal_size_limit)
                .map(Arc::new)
                .map_err(|error| MapReduceError::InvalidInitialValue {
                    position,
                    reason: error.reason().into_string(),
                })
        })
        .collect()
}
