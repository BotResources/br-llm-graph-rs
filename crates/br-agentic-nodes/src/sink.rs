use br_llm_clients_core::{BoxFuture, StreamSink};
use br_llm_messages::ResponseDelta;

pub(crate) struct DiscardSink;

impl StreamSink for DiscardSink {
    fn send<'a>(&'a mut self, _delta: ResponseDelta) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}
