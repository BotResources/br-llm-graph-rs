use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use super::core::Core;
use super::{Event, EventEndError};
use crate::{Fields, InstanceId};

pub struct EventEnd<F: Fields> {
    pub(super) core: Arc<Core<F>>,
}

impl<F: Fields> EventEnd<F> {
    pub(super) fn new(core: Arc<Core<F>>) -> Self {
        Self { core }
    }

    pub fn id(&self) -> InstanceId {
        self.core.id()
    }

    pub async fn recv(&mut self) -> Result<Event<F>, EventEndError> {
        Recv { core: &self.core }.await
    }
}

struct Recv<'a, F: Fields> {
    core: &'a Core<F>,
}

impl<F: Fields> Future for Recv<'_, F> {
    type Output = Result<Event<F>, EventEndError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.core.poll_recv(cx)
    }
}

impl<F: Fields> Drop for Recv<'_, F> {
    fn drop(&mut self) {
        self.core.forget_event_waker();
    }
}

impl<F: Fields> Drop for EventEnd<F> {
    fn drop(&mut self) {
        self.core.event_end_dropped();
    }
}

impl<F: Fields> fmt::Debug for EventEnd<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventEnd").field("id", &self.id()).finish()
    }
}
