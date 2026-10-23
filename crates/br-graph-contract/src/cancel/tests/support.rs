use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;

pub(super) fn poll_once<T>(future: &mut Pin<&mut impl Future<Output = T>>) -> Poll<T> {
    future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
}

pub(super) fn poll_with<T>(
    future: &mut Pin<&mut impl Future<Output = T>>,
    waker: &Waker,
) -> Poll<T> {
    future.as_mut().poll(&mut Context::from_waker(waker))
}

#[derive(Default)]
pub(super) struct Count(pub(super) AtomicUsize);

impl Count {
    pub(super) fn value(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

impl Wake for Count {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

pub(super) fn counting_waker() -> (Arc<Count>, Waker) {
    let count = Arc::new(Count::default());
    let waker = Waker::from(Arc::clone(&count));
    (count, waker)
}

pub(super) fn finishes_within<F>(work: F)
where
    F: FnOnce() + Send + 'static,
{
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        work();
        let _ = done.send(());
    });
    assert!(
        finished.recv_timeout(Duration::from_secs(10)).is_ok(),
        "the work did not finish: deadlock"
    );
}
