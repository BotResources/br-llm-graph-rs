use std::sync::Arc;

use br_graph_contract::{
    Event, EventEnd, EventEndError, Fields, Input, InputClosed, InputEnd, InstanceId,
};
use uuid::Uuid;

use super::MapReduceError;
use super::creation::Creation;
use super::work::{ChildEnd, Halt, Work};
use crate::panic_text::caught;
use crate::signals::Signals;

async fn listen<G: Fields>(
    position: usize,
    input: InputEnd<G>,
    mut events: EventEnd<G>,
) -> ChildEnd<G> {
    let received = events.recv().await;
    drop(input);
    (position, received)
}

impl<F: Fields, G: Fields> Work<'_, F, G> {
    pub(super) async fn start(&mut self, signals: &mut Signals) -> Result<(), Halt> {
        let position = self.next;
        let id = InstanceId::new(Uuid::now_v7()).map_err(|error| {
            Halt::Fault(MapReduceError::InstanceNotCreated {
                position,
                cause: Box::new(error),
            })
        })?;
        let rules = self.rules;
        let mut creation = Creation::begin(&*rules.internal_node, self.scope.view(), id);
        let created = loop {
            tokio::select! {
                biased;
                () = &mut signals.dropped => return Err(Halt::Dropped),
                () = &mut signals.cancel => return Err(Halt::Cancel),
                joined = self.children.join_next(), if !self.children.is_empty() => match joined {
                    Some(Ok((finished, received))) => self.receive(finished, received)?,
                    Some(Err(_)) | None => return Err(Halt::Dropped),
                },
                created = &mut creation => break created,
            }
        };
        let instance = match created {
            Ok(Ok(instance)) => instance,
            Ok(Err(error)) => {
                return Err(Halt::Fault(MapReduceError::InstanceNotCreated {
                    position,
                    cause: error.into_cause(),
                }));
            }
            Err(text) => {
                return Err(Halt::Fault(MapReduceError::InstanceNotCreated {
                    position,
                    cause: text.into(),
                }));
            }
        };
        if instance.id() != id {
            return Err(Halt::Fault(MapReduceError::InstanceIdMismatch {
                position,
                expected: id.to_string(),
                returned: instance.id().to_string(),
            }));
        }
        let (input, events) = instance.into_parts();
        input
            .give(Input::new(Arc::clone(&self.states[position])))
            .map_err(|InputClosed| Halt::Fault(MapReduceError::InputRefused { position }))?;
        self.children.spawn(listen(position, input, events));
        self.next += 1;
        Ok(())
    }

    pub(super) fn receive(
        &mut self,
        position: usize,
        received: Result<Event<G>, EventEndError>,
    ) -> Result<(), Halt> {
        let fault = match received {
            Ok(Event::Result(update)) => {
                let state = &self.states[position];
                match caught(|| state.apply(update)) {
                    Ok(Ok(state)) => {
                        self.states[position] = Arc::new(state);
                        return Ok(());
                    }
                    Ok(Err(error)) => MapReduceError::UpdateRejected {
                        position,
                        reason: error.to_string(),
                    },
                    Err(reason) => MapReduceError::UpdateRejected { position, reason },
                }
            }
            Ok(Event::Failed(cause)) => MapReduceError::InternalFailed { position, cause },
            Ok(Event::Refused(reason)) => MapReduceError::InternalRefused {
                position,
                reason: reason.into_string(),
            },
            Ok(Event::Cancelled) if self.scope.is_triggered() => return Err(Halt::Cancel),
            Ok(Event::Cancelled) => MapReduceError::InternalCancelled { position },
            Err(EventEndError::Vanished | EventEndError::Finished) => {
                MapReduceError::InternalVanished { position }
            }
        };
        Err(Halt::Fault(fault))
    }
}
