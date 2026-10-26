use std::future::Future;
use std::mem;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::task::{Context, Poll};

use br_graph_contract::{
    CancelView, CreateError, CreateFuture, Fields, Instance, InstanceId, Node,
};

use crate::panic_text::{caught, panic_text};

pub(super) enum Creation<'a, G: Fields> {
    Running(CreateFuture<'a, G>),
    Panicked(String),
    Done,
}

impl<'a, G: Fields> Creation<'a, G> {
    pub(super) fn begin(node: &'a dyn Node<G>, view: CancelView, id: InstanceId) -> Self {
        match caught(|| node.create(view, id)) {
            Ok(future) => Self::Running(future),
            Err(text) => Self::Panicked(text),
        }
    }
}

impl<G: Fields> Future for Creation<'_, G> {
    type Output = Result<Result<Instance<G>, CreateError>, String>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let finished = match &mut *self {
            Self::Running(future) => {
                match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(cx))) {
                    Ok(Poll::Ready(created)) => Ok(created),
                    Ok(Poll::Pending) => return Poll::Pending,
                    Err(payload) => Err(panic_text(payload)),
                }
            }
            Self::Panicked(text) => Err(mem::take(text)),
            Self::Done => return Poll::Pending,
        };
        *self = Self::Done;
        Poll::Ready(finished)
    }
}

#[cfg(test)]
mod tests;
