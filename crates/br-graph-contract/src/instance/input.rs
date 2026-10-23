use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

use crate::{Fields, State};

pub struct Input<F: Fields> {
    state: Arc<State<F>>,
}

impl<F: Fields> Input<F> {
    pub fn new(state: Arc<State<F>>) -> Self {
        Self { state }
    }

    pub fn state(&self) -> &State<F> {
        &self.state
    }

    pub fn into_state(self) -> Arc<State<F>> {
        self.state
    }
}

impl<F: Fields> Clone for Input<F> {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

impl<F: Fields> Deref for Input<F> {
    type Target = State<F>;

    fn deref(&self) -> &State<F> {
        &self.state
    }
}

impl<F: Fields> From<Arc<State<F>>> for Input<F> {
    fn from(state: Arc<State<F>>) -> Self {
        Self::new(state)
    }
}

impl<F: Fields> fmt::Debug for Input<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Input")
            .field("state", &*self.state)
            .finish()
    }
}
