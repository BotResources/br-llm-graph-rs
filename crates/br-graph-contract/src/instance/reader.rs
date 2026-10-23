use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use super::Input;
use super::core::Core;
use crate::{Fields, InstanceId};

pub struct InputReader<F: Fields> {
    pub(super) core: Arc<Core<F>>,
}

impl<F: Fields> InputReader<F> {
    pub(super) fn new(core: Arc<Core<F>>) -> Self {
        Self { core }
    }

    pub fn id(&self) -> InstanceId {
        self.core.id()
    }

    pub async fn next(&mut self) -> Option<Input<F>> {
        Next { core: &self.core }.await
    }

    pub fn close(&self) {
        self.core.close_input();
    }

    pub fn drain(&mut self) -> Vec<Input<F>> {
        self.core.drain()
    }
}

struct Next<'a, F: Fields> {
    core: &'a Core<F>,
}

impl<F: Fields> Future for Next<'_, F> {
    type Output = Option<Input<F>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.core.poll_next(cx)
    }
}

impl<F: Fields> Drop for Next<'_, F> {
    fn drop(&mut self) {
        self.core.forget_reader_waker();
    }
}

impl<F: Fields> Drop for InputReader<F> {
    fn drop(&mut self) {
        self.core.close_input();
    }
}

impl<F: Fields> fmt::Debug for InputReader<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InputReader")
            .field("id", &self.id())
            .finish()
    }
}
