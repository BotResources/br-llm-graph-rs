use br_graph_contract::{
    CancelView, Dropped, Event, EventSender, Fields, Input, InputReader, Triggered,
};

pub(crate) struct Signals {
    pub(crate) cancel: Triggered,
    pub(crate) dropped: Dropped,
}

pub(crate) enum Awaited<F: Fields> {
    Input(Input<F>),
    Cancelled,
    Gone,
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
    ) -> Awaited<F> {
        tokio::select! {
            biased;
            () = &mut self.dropped => Awaited::Gone,
            () = &mut self.cancel => Awaited::Cancelled,
            next = reader.next() => next.map_or(Awaited::Gone, Awaited::Input),
        }
    }
}

pub(crate) fn settle<F: Fields>(
    reader: &mut InputReader<F>,
    sender: EventSender<F>,
    event: Event<F>,
) {
    reader.close();
    drop(reader.drain());
    sender.finish(event);
}
