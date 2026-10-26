use std::sync::Arc;

use br_graph_contract::{CancelView, EventEnd, Fields, InstanceId, Node};
use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use super::guard::{AbortGuard, AbortOnDrop};
use super::message::{CreationFailure, Message};
use crate::panic_text::panic_reason;

pub(super) fn mint_id() -> Result<InstanceId, String> {
    InstanceId::new(Uuid::now_v7()).map_err(|error| error.to_string())
}

pub(super) fn spawn_creation<F: Fields>(
    node: Arc<dyn Node<F>>,
    name: String,
    view: CancelView,
    id: InstanceId,
    sender: UnboundedSender<Message<F>>,
) -> AbortGuard {
    let task = tokio::spawn(async move {
        let mut creating = AbortOnDrop(tokio::spawn(async move { node.create(view, id).await }));
        let result = match (&mut creating.0).await {
            Ok(Ok(instance)) => Ok(instance),
            Ok(Err(error)) => Err(CreationFailure {
                reason: error.cause().to_string(),
                cause: Some(error.into_cause()),
            }),
            Err(error) if error.is_panic() => Err(CreationFailure {
                reason: panic_reason(&*error.into_panic()),
                cause: None,
            }),
            Err(_) => Err(CreationFailure {
                reason: "the creation was cancelled".to_owned(),
                cause: None,
            }),
        };
        let _ = sender.send(Message::Created {
            node: name,
            id,
            result,
        });
    });
    AbortGuard::new(task.abort_handle())
}

pub(super) fn spawn_forwarder<F: Fields>(
    mut events: EventEnd<F>,
    name: String,
    id: InstanceId,
    sender: UnboundedSender<Message<F>>,
) -> AbortGuard {
    let task = tokio::spawn(async move {
        let result = events.recv().await;
        let _ = sender.send(Message::Ended {
            node: name,
            id,
            result,
        });
        std::future::pending::<()>().await;
    });
    AbortGuard::new(task.abort_handle())
}
