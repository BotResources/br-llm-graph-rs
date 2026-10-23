use std::fmt;
use std::sync::Arc;

use super::core::Core;
use super::{Input, InputClosed};
use crate::{Fields, InstanceId};

pub struct InputEnd<F: Fields> {
    pub(super) core: Arc<Core<F>>,
}

impl<F: Fields> InputEnd<F> {
    pub(super) fn new(core: Arc<Core<F>>) -> Self {
        Self { core }
    }

    pub fn id(&self) -> InstanceId {
        self.core.id()
    }

    pub fn give(&self, input: Input<F>) -> Result<(), InputClosed> {
        self.core.give(input)
    }
}

impl<F: Fields> Drop for InputEnd<F> {
    fn drop(&mut self) {
        self.core.executor_dropped();
    }
}

impl<F: Fields> fmt::Debug for InputEnd<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InputEnd").field("id", &self.id()).finish()
    }
}
