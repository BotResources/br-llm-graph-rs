use std::num::NonZeroUsize;
use std::sync::Arc;

use br_graph_contract::{CancelView, Event, EventSender, Fields, InputReader};

use super::Rules;
use super::work::{self, Outcome};
use crate::signals::{Awaited, Signals, settle};

pub(super) async fn run<F: Fields, G: Fields>(
    rules: Arc<Rules<F, G>>,
    internal_limit: NonZeroUsize,
    mut reader: InputReader<F>,
    sender: EventSender<F>,
    cancel: CancelView,
) {
    let mut signals = Signals::new(&cancel, &sender);
    let first = match signals.next_input(&mut reader).await {
        Awaited::Gone => return,
        Awaited::Cancelled => return settle(&mut reader, sender, Event::Cancelled),
        Awaited::Input(input) => input,
    };
    reader.close();
    let last = reader.drain().pop().unwrap_or(first);
    let outcome = work::run(&rules, internal_limit, last.state(), &cancel, &mut signals).await;
    match outcome {
        Outcome::Result(update) => settle(&mut reader, sender, Event::Result(update)),
        Outcome::Cancelled => settle(&mut reader, sender, Event::Cancelled),
        Outcome::Failed(error) => settle(&mut reader, sender, Event::failed(error)),
        Outcome::Dropped => {}
    }
}
