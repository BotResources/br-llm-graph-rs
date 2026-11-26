use std::future::poll_fn;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::Poll;

use crate::memory::wakers::Wakers;
use crate::sandbox::BoxId;

#[derive(Default)]
struct HoldState {
    made: Option<BoxId>,
    proceed: bool,
}

#[derive(Default)]
struct Shared {
    state: Mutex<HoldState>,
    made: Wakers,
    proceed: Wakers,
}

#[derive(Clone, Default)]
pub struct OpenHold {
    shared: Arc<Shared>,
}

impl OpenHold {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub async fn box_made(&self) -> BoxId {
        poll_fn(|cx| {
            let state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            match &state.made {
                Some(sandbox) => Poll::Ready(sandbox.clone()),
                None => {
                    self.shared.made.register(cx.waker());
                    Poll::Pending
                }
            }
        })
        .await
    }

    pub fn proceed(&self) {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .proceed = true;
        self.shared.proceed.wake_all();
    }

    pub(super) fn reach(&self, sandbox: BoxId) {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .made = Some(sandbox);
        self.shared.made.wake_all();
    }

    pub(super) async fn proceeded(&self) {
        poll_fn(|cx| {
            let state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if state.proceed {
                Poll::Ready(())
            } else {
                self.shared.proceed.register(cx.waker());
                Poll::Pending
            }
        })
        .await
    }
}
