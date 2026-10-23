use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Wake, Waker};

#[derive(Default)]
pub struct Wakes(AtomicUsize);

impl Wakes {
    pub fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

impl Wake for Wakes {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

pub fn counting_waker() -> (Arc<Wakes>, Waker) {
    let wakes = Arc::new(Wakes::default());
    let waker = Waker::from(Arc::clone(&wakes));
    (wakes, waker)
}

pub fn poll_with<T>(future: Pin<&mut impl Future<Output = T>>, waker: &Waker) -> Poll<T> {
    future.poll(&mut Context::from_waker(waker))
}
