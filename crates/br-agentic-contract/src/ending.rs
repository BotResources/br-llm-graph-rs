use br_graph_contract::{Cause, Event, Fields, InputReader};

use crate::session::Session;

pub async fn result<F: Fields>(
    reader: &InputReader<F>,
    session: &mut Session<'_>,
    update: F::Update,
) -> Event<F> {
    session.discard_all().await;
    reader.close();
    Event::Result(update)
}

pub async fn failed<F: Fields>(
    reader: &InputReader<F>,
    session: &mut Session<'_>,
    cause: impl Into<Cause>,
) -> Event<F> {
    session.discard_all().await;
    reader.close();
    Event::failed(cause)
}

pub async fn cancelled<F: Fields>(reader: &InputReader<F>, session: &mut Session<'_>) -> Event<F> {
    session.discard_all().await;
    reader.close();
    Event::Cancelled
}
