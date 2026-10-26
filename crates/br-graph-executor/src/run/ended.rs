use br_graph_contract::{Event, EventEndError, Fields, InstanceId};

use super::Run;
use crate::panic_text::display_of;
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
        slot.end_instance();
        match result {
            Ok(Event::Result(update)) => {
                self.report_event(&node, id, ObservedEvent::Result);
                self.result_step(node, update).await;
            }
            Ok(Event::Failed(cause)) => {
                self.report_event(&node, id, ObservedEvent::Failed);
                let failure = RunFailure::NodeFailed {
                    node: node.clone(),
                    reason: display_of(&*cause),
                    cause: Some(cause),
                };
                self.fail_node(&node, failure);
            }
            Ok(Event::Refused(reason)) => {
                self.report_event(&node, id, ObservedEvent::Refused);
                let failure = RunFailure::NodeRefusedInput {
                    node: node.clone(),
                    reason: reason.into_string(),
                };
                self.fail_node(&node, failure);
            }
            Ok(Event::Cancelled) => {
                self.report_event(&node, id, ObservedEvent::Cancelled);
                if self.cancel.handle().is_triggered() {
                    self.leave_untaken(&node);
                } else {
                    let failure = RunFailure::NodeFailed {
                        node: node.clone(),
                        reason: "cancelled without a cancel request".to_owned(),
                        cause: None,
                    };
                    self.fail_node(&node, failure);
                }
            }
            Err(EventEndError::Vanished) => {
                let failure = vanished(&node, "the node side ended without a terminal event");
                self.fail_node(&node, failure);
            }
            Err(EventEndError::Finished) => {
                let failure = vanished(&node, "the event end finished without an event");
                self.fail_node(&node, failure);
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
}

fn vanished(node: &str, reason: &str) -> RunFailure {
    RunFailure::NodeVanished {
        node: node.to_owned(),
        reason: reason.to_owned(),
    }
}
