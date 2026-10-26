use std::num::NonZeroUsize;
use std::sync::Arc;

use br_graph_contract::{Fields, State};

use super::{MapReduceError, Rules};
use crate::panic_text::caught;

pub(super) fn states<F: Fields, G: Fields>(
    rules: &Rules<F, G>,
    internal_size_limit: NonZeroUsize,
    state: &State<F>,
) -> Result<Vec<Arc<State<G>>>, MapReduceError> {
    (rules.map)(state)
        .into_iter()
        .enumerate()
        .map(|(position, fields)| {
            match caught(|| State::create_with_limit(fields, internal_size_limit)) {
                Ok(Ok(state)) => Ok(Arc::new(state)),
                Ok(Err(error)) => Err(MapReduceError::InvalidInitialValue {
                    position,
                    reason: error.reason().into_string(),
                }),
                Err(reason) => Err(MapReduceError::InvalidInitialValue { position, reason }),
            }
        })
        .collect()
}
