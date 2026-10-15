//! A model call and its sink: where the stream events of a call go while
//! the reply is produced.

use br_llm_clients_core::{BoxFuture, Client, ClientRequest, StreamSink};
use br_llm_messages::{Step, StreamEvent};

use crate::error::GraphError;
use crate::graph::Context;
use crate::observe::Observer;
use crate::origin::Origin;
use crate::value::Key;

/// Sends each stream event of a call to the observer, with the origin of the
/// calling node and its conversation key.
struct ObserverSink<'a> {
    observer: &'a dyn Observer,
    origin: &'a Origin,
    key: &'a Key,
}

impl StreamSink for ObserverSink<'_> {
    fn send<'a>(&'a mut self, event: StreamEvent) -> BoxFuture<'a, ()> {
        Box::pin(async move { self.observer.stream(self.origin, self.key, &event) })
    }
}

/// Calls `client` with `request`, with a sink that sends each stream event to
/// the observer of `ctx`, under the occurrence of `ctx` and the conversation
/// key `key`. An error of the client gives `Model`, carrying the
/// `ClientError`; the call's report is not kept.
pub async fn complete(
    client: &dyn Client,
    request: &ClientRequest,
    ctx: &Context,
    key: &Key,
) -> Result<Step, GraphError> {
    let mut sink = ObserverSink {
        observer: ctx.observer.as_ref(),
        origin: ctx.origin(),
        key,
    };
    let (_report, result) = client.call(request, &mut sink).await.into_parts();
    result.map_err(GraphError::Model)
}
