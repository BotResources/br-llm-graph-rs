use std::any::Any;
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};

use br_graph_contract::{CancelView, Dropped, EventSender, Fields, Input, InputReader, Triggered};
use futures_util::FutureExt;

use crate::fault::{Fault, Stop};

pub(crate) struct Signals {
    cancel: Triggered,
    dropped: Dropped,
}

pub(crate) enum Raced<T> {
    Gone,
    Cancelled,
    Done(T),
}

impl Signals {
    pub(crate) fn new<F: Fields>(cancel: &CancelView, sender: &EventSender<F>) -> Self {
        Self {
            cancel: cancel.triggered(),
            dropped: sender.dropped(),
        }
    }

    pub(crate) async fn next_input<F: Fields>(
        &mut self,
        reader: &mut InputReader<F>,
    ) -> Raced<Input<F>> {
        match self.race(reader.next()).await {
            Raced::Done(Some(input)) => Raced::Done(input),
            Raced::Done(None) | Raced::Gone => Raced::Gone,
            Raced::Cancelled => Raced::Cancelled,
        }
    }

    pub(crate) async fn race<T>(&mut self, work: impl Future<Output = T>) -> Raced<T> {
        tokio::select! {
            biased;
            () = &mut self.dropped => Raced::Gone,
            () = &mut self.cancel => Raced::Cancelled,
            done = work => Raced::Done(done),
        }
    }
}

fn text_of(payload: &(dyn Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        return (*text).to_owned();
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    "panic without a text message".to_owned()
}

fn release(payload: Box<dyn Any + Send>) {
    if let Err(second) = catch_unwind(AssertUnwindSafe(move || drop(payload))) {
        std::mem::forget(second);
    }
}

pub(crate) async fn guarded<T>(work: impl Future<Output = Result<T, Stop>>) -> Result<T, Stop> {
    match AssertUnwindSafe(work).catch_unwind().await {
        Ok(result) => result,
        Err(payload) => {
            let text = text_of(&*payload);
            release(payload);
            Err(Stop::Fault(Fault::Panic(text)))
        }
    }
}
