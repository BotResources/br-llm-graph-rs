//! A model call and its sink: where the stream events of a call go while
//! the reply is produced.

use br_llm_clients_core::{BoxFuture, Client, ClientRequest, StreamSink};
use br_llm_messages::{Step, StreamEvent};

use crate::error::GraphError;
use crate::graph::Context;
use crate::observe::Observer;
use crate::origin::Origin;
use crate::react::turn_writer::TurnWriter;
use crate::value::Key;

/// Sends each stream event of a call to the observer, with the origin of the
/// calling node and its conversation key; for a durable agent, pushes it
/// first into the agent's open turn, until the store fails, keeping its
/// error.
struct NodeSink<'a> {
    observer: &'a dyn Observer,
    origin: &'a Origin,
    key: &'a Key,
    turn: Option<&'a TurnWriter<'a>>,
    failed: Option<GraphError>,
}

impl StreamSink for NodeSink<'_> {
    fn send<'a>(&'a mut self, event: StreamEvent) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            if let Some(turn) = self.turn
                && self.failed.is_none()
                && let Err(error) = turn.push_event(&event).await
            {
                self.failed = Some(error);
            }
            self.observer.stream(self.origin, self.key, &event);
        })
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
    complete_in(client, request, ctx, key, None).await
}

/// `complete`, the sink also pushing each event into `turn`, the open turn
/// of a durable agent, before the observer receives it. An error of the
/// store gives `Thread`, before any error of the client: the events after it
/// still reach the observer, never the store.
pub(crate) async fn complete_in(
    client: &dyn Client,
    request: &ClientRequest,
    ctx: &Context,
    key: &Key,
    turn: Option<&TurnWriter<'_>>,
) -> Result<Step, GraphError> {
    let mut sink = NodeSink {
        observer: ctx.observer.as_ref(),
        origin: ctx.origin(),
        key,
        turn,
        failed: None,
    };
    let (_report, result) = client.call(request, &mut sink).await.into_parts();
    if let Some(error) = sink.failed {
        return Err(error);
    }
    result.map_err(GraphError::Model)
}
