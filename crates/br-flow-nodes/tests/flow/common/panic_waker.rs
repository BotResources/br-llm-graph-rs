use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Wake, Waker};

struct PanicWake;

impl Wake for PanicWake {
    fn wake(self: Arc<Self>) {
        panic!("the registered waker panics when it is woken");
    }

    fn wake_by_ref(self: &Arc<Self>) {
        panic!("the registered waker panics when it is woken");
    }
}

pub fn register_panicking_waker<Fut: Future + Unpin>(mut future: Fut) -> Fut {
    let waker = Waker::from(Arc::new(PanicWake));
    let mut cx = Context::from_waker(&waker);
    let _ = Pin::new(&mut future).poll(&mut cx);
    future
}
