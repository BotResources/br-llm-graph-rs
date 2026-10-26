use std::sync::Arc;

use br_graph_contract::{Fields, Instance, InstanceId, State};

use super::Run;
use super::instances::spawn_forwarder;
use super::message::CreationFailure;
use crate::{Observation, RunFailure};

impl<F: Fields> Run<F> {
    pub(super) fn on_created(
        &mut self,
        node: String,
        id: InstanceId,
        result: Result<Instance<F>, CreationFailure>,
    ) {
        let Some(slot) = self.slots.get_mut(&node) else {
            return;
        };
        let Some(first_input) = slot.first_input_of_creation(id) else {
            return;
        };
        let failure = match result {
            Ok(instance) if instance.id() == id => {
                self.settle_created(&node, instance, first_input);
                return;
            }
            Ok(_) => CreationFailure {
                reason: "the instance does not carry the instance id it was given".to_owned(),
                cause: None,
            },
            Err(failure) => failure,
        };
        slot.end_instance();
        slot.hold();
        self.fail_node(
            &node,
            RunFailure::NodeCouldNotStart {
                node: node.clone(),
                reason: failure.reason,
                cause: failure.cause,
            },
        );
    }

    fn settle_created(&mut self, node: &str, instance: Instance<F>, first_input: Arc<State<F>>) {
        let id = instance.id();
        let (input, events) = instance.into_parts();
        let forwarder = spawn_forwarder(events, node.to_owned(), id, self.sender.clone());
        let Some(slot) = self.slots.get_mut(node) else {
            return;
        };
        slot.become_running(id, input, forwarder);
        let (kind, subkind) = (slot.kind().to_owned(), slot.subkind().to_owned());
        self.observing.tell(|| Observation::InstanceCreated {
            instance: id,
            node: node.to_owned(),
            kind,
            subkind,
        });
        if !self.give_input(node, first_input) {
            if let Some(slot) = self.slots.get_mut(node) {
                slot.hold();
            }
            return;
        }
        let has_held = self.slots.get(node).is_some_and(|slot| slot.is_held());
        if has_held && !self.is_stopping() {
            self.give_held(node);
        }
    }
}
