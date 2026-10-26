use br_graph_contract::{Event, EventSender, Fields, InputReader};

pub(crate) fn settle<F: Fields>(
    reader: &mut InputReader<F>,
    sender: EventSender<F>,
    event: Event<F>,
) {
    reader.close();
    drop(reader.drain());
    sender.finish(event);
}
