use std::sync::Arc;

use br_graph_contract::{Fields, Instance, InstanceId, State};

use super::Run;
use super::guard::AbortGuard;
use super::instances::spawn_forwarder;
use super::message::CreationFailure;
use super::slot::Live;
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
        let first_input = match &slot.live {
            Live::Creating {
                id: live,
                first_input,
                ..
            } if *live == id => Arc::clone(first_input),
            _ => return,
        };
        let failure = match result {
            Ok(instance) if instance.id() == id => {
                self.settle_created(node, instance, first_input);
                return;
            }
            Ok(_) => CreationFailure {
                reason: "the instance does not carry the instance id it was given".to_owned(),
                cause: None,
            },
            Err(failure) => failure,
        };
        slot.live = Live::Absent;
        slot.held = true;
        slot.untaken = true;
        self.fail(RunFailure::NodeCouldNotStart {
            node,
            reason: failure.reason,
            cause: failure.cause,
        });
    }

    fn settle_created(&mut self, node: String, instance: Instance<F>, first_input: Arc<State<F>>) {
        let id = instance.id();
        let (input, events) = instance.into_parts();
        let forwarder: AbortGuard = spawn_forwarder(events, node.clone(), id, self.sender.clone());
        let Some(slot) = self.slots.get_mut(&node) else {
            return;
        };
        slot.live = Live::Running {
            _forwarder: forwarder,
            id,
            input,
        };
        let (kind, subkind) = (slot.kind.clone(), slot.subkind.clone());
        self.observing.tell(|| Observation::InstanceCreated {
            instance: id,
            node: node.clone(),
            kind,
            subkind,
        });
        if !self.give_input(&node, first_input) {
            if let Some(slot) = self.slots.get_mut(&node) {
                slot.held = true;
            }
            return;
        }
        let has_held = self.slots.get(&node).is_some_and(|slot| slot.held);
        if has_held && !self.is_stopping() {
            let current = Arc::clone(&self.state);
            if self.give_input(&node, current) {
                self.clear_held(&node);
            }
        }
    }
}
