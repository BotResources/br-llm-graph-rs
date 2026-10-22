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

/// What a durable agent's sink does with the events of a step, besides
/// sending them to the observer.
pub(crate) enum ToThread<'a> {
    /// Pushes each into the open turn as it comes.
    Live(&'a TurnWriter<'a>),
    /// Holds them, to be pushed once the step is checked: with a structured
    /// output mode, a step asked for again is never stored.
    Held,
}

/// What a model call gave: its step or its error, the events held for the
/// thread, and whether any event reached the thread.
pub(crate) struct Streamed {
    pub(crate) result: Result<Step, GraphError>,
    /// The events of the step, in order, when they were held.
    pub(crate) held: Vec<StreamEvent>,
    /// Whether the store took at least one event of the call.
    pub(crate) reached: bool,
}

/// Sends each stream event of a call to the observer, with the origin of the
/// calling node and its conversation key; for a durable agent, first pushes
/// it into the agent's open turn, until the store fails, keeping its error,
/// or holds it.
struct NodeSink<'a> {
    observer: &'a dyn Observer,
    origin: &'a Origin,
    key: &'a Key,
    thread: Option<ToThread<'a>>,
    held: Vec<StreamEvent>,
    reached: bool,
    failed: Option<GraphError>,
}

impl StreamSink for NodeSink<'_> {
    fn send<'a>(&'a mut self, event: StreamEvent) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            match &self.thread {
                Some(ToThread::Live(turn)) if self.failed.is_none() => {
                    match turn.push_event(&event).await {
                        Ok(()) => self.reached = true,
                        Err(error) => self.failed = Some(error),
                    }
                }
                Some(ToThread::Held) => self.held.push(event.clone()),
                Some(ToThread::Live(_)) | None => {}
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
    stream(client, request, ctx, key, None).await.result
}

/// `complete`, the sink also giving each event to the thread of a durable
/// agent before the observer receives it: pushed into the open turn as it
/// comes, or held. An error of the store gives `Thread`, before any error of
/// the client: the events after it still reach the observer, never the
/// store.
pub(crate) async fn stream(
    client: &dyn Client,
    request: &ClientRequest,
    ctx: &Context,
    key: &Key,
    thread: Option<ToThread<'_>>,
) -> Streamed {
    let mut sink = NodeSink {
        observer: ctx.observer.as_ref(),
        origin: ctx.origin(),
        key,
        thread,
        held: Vec::new(),
        reached: false,
        failed: None,
    };
    let (_report, result) = client.call(request, &mut sink).await.into_parts();
    let result = match sink.failed {
        Some(error) => Err(error),
        None => result.map_err(GraphError::Model),
    };
    Streamed {
        result,
        held: sink.held,
        reached: sink.reached,
    }
}
