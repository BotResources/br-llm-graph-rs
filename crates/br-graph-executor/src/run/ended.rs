use br_graph_contract::{Cause, Event, EventEndError, Fields, InstanceId};

use super::Run;
use super::slot::Live;
use crate::{Observation, ObservedEvent, RunFailure};

impl<F: Fields> Run<F> {
    pub(super) async fn on_ended(
        &mut self,
        node: String,
        id: InstanceId,
        result: Result<Event<F>, EventEndError>,
    ) {
        let Some(slot) = self.slots.get_mut(&node) else {
            return;
        };
        if slot.running_id() != Some(id) {
            return;
        }
        slot.live = Live::Absent;
        match result {
            Ok(Event::Result(update)) => {
                self.report_event(&node, id, ObservedEvent::Result);
                self.result_step(node, update).await;
            }
            Ok(Event::Failed(cause)) => {
                self.report_event(&node, id, ObservedEvent::Failed);
                let reason = cause.to_string();
                self.leave_untaken(&node);
                self.fail(RunFailure::NodeFailed {
                    node,
                    reason,
                    cause,
                });
            }
            Ok(Event::Refused(reason)) => {
                self.report_event(&node, id, ObservedEvent::Refused);
                self.leave_untaken(&node);
                self.fail(RunFailure::NodeRefusedInput {
                    node,
                    reason: reason.into_string(),
                });
            }
            Ok(Event::Cancelled) => {
                self.report_event(&node, id, ObservedEvent::Cancelled);
                self.leave_untaken(&node);
                if !self.cancel.handle().is_triggered() {
                    let reason = "cancelled without a cancel request".to_owned();
                    self.fail(RunFailure::NodeFailed {
                        node,
                        cause: Cause::from(reason.as_str()),
                        reason,
                    });
                }
            }
            Err(EventEndError::Vanished) => {
                self.leave_untaken(&node);
                self.fail(RunFailure::NodeVanished {
                    node,
                    reason: "the node side ended without a terminal event".to_owned(),
                });
            }
            Err(EventEndError::Finished) => {
                self.leave_untaken(&node);
                self.fail(RunFailure::NodeVanished {
                    node,
                    reason: "the event end finished without an event".to_owned(),
                });
            }
        }
    }

    fn report_event(&self, node: &str, instance: InstanceId, event: ObservedEvent) {
        self.observing.tell(|| Observation::EventHandled {
            node: node.to_owned(),
            instance,
            event,
        });
    }

    fn leave_untaken(&mut self, node: &str) {
        if let Some(slot) = self.slots.get_mut(node) {
            slot.untaken = true;
        }
    }
}
