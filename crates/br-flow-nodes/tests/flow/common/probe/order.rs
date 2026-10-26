use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};

use super::watch::locked;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Triggered(usize),
    Dropped(usize),
    Delivered,
}

#[derive(Default)]
pub struct Log {
    marks: Mutex<Vec<Mark>>,
}

impl Log {
    pub fn index_of(&self, mark: Mark) -> Option<usize> {
        locked(&self.marks).iter().position(|seen| *seen == mark)
    }

    fn record(&self, mark: Mark) {
        locked(&self.marks).push(mark);
    }
}

struct Notice {
    log: Arc<Log>,
    mark: Mark,
    waker: Waker,
}

impl Wake for Notice {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.log.record(self.mark);
        self.waker.wake_by_ref();
    }
}

pub struct Witness<Fut> {
    inner: Fut,
    log: Arc<Log>,
    mark: Mark,
}

impl<Fut> Witness<Fut> {
    pub fn new(inner: Fut, log: &Arc<Log>, mark: Mark) -> Self {
        Self {
            inner,
            log: Arc::clone(log),
            mark,
        }
    }
}

impl<Fut: Future + Unpin> Future for Witness<Fut> {
    type Output = Fut::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let waker = Waker::from(Arc::new(Notice {
            log: Arc::clone(&self.log),
            mark: self.mark,
            waker: cx.waker().clone(),
        }));
        let mut witnessed = Context::from_waker(&waker);
        Pin::new(&mut self.inner).poll(&mut witnessed)
    }
}
