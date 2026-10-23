use crate::{Cause, Fields, State, Target};

pub trait Edge<F: Fields>: Send + Sync {
    fn name(&self) -> &str;

    fn source(&self) -> &str;

    fn targets(&self) -> &[Target];

    fn evaluate(&self, state: &State<F>) -> Result<Vec<Target>, Cause>;
}
