use std::sync::Arc;

use br_graph_contract::{Fields, Input, Node, State};

use super::Run;
use super::guard::AbortGuard;
use super::instances::{mint_id, spawn_creation};
use super::slot::Live;
use crate::{Observation, RunFailure};

impl<F: Fields> Run<F> {
    pub(super) fn hand_over(&mut self, targets: &[String]) {
        let mut order: Vec<String> = targets.to_vec();
        for (name, slot) in &self.slots {
            if slot.held && !order.contains(name) {
                order.push(name.clone());
            }
        }
        for name in order {
            if self.is_stopping() {
                return;
            }
            self.hand_to(&name);
        }
    }

    fn hand_to(&mut self, name: &str) {
        let state = Arc::clone(&self.state);
        let Some(slot) = self.slots.get(name) else {
            return;
        };
        if !slot.held {
            return;
        }
        match &slot.live {
            Live::Creating { .. } => {}
            Live::Running { .. } => {
                if self.give_input(name, state) {
                    self.clear_held(name);
                }
            }
            Live::Absent => {
                let node = Arc::clone(&slot.node);
                self.clear_held(name);
                self.begin_creation(name, node, state);
            }
        }
    }

    pub(super) fn clear_held(&mut self, name: &str) {
        if let Some(slot) = self.slots.get_mut(name) {
            slot.held = false;
        }
    }

    pub(super) fn give_input(&self, name: &str, state: Arc<State<F>>) -> bool {
        let Some(slot) = self.slots.get(name) else {
            return false;
        };
        let Live::Running { id, input, .. } = &slot.live else {
            return false;
        };
        if input.give(Input::new(state)).is_err() {
            return false;
        }
        let instance = *id;
        self.observing.tell(|| Observation::InputHandedOver {
            node: name.to_owned(),
            instance,
        });
        true
    }

    fn begin_creation(&mut self, name: &str, node: Arc<dyn Node<F>>, first_input: Arc<State<F>>) {
        let id = match mint_id() {
            Ok(id) => id,
            Err(reason) => {
                self.creation_impossible(name, reason);
                return;
            }
        };
        let creation: AbortGuard = spawn_creation(
            node,
            name.to_owned(),
            self.cancel.view(),
            id,
            self.sender.clone(),
        );
        if let Some(slot) = self.slots.get_mut(name) {
            slot.live = Live::Creating {
                id,
                first_input,
                _creation: creation,
            };
        }
    }

    fn creation_impossible(&mut self, name: &str, reason: String) {
        if let Some(slot) = self.slots.get_mut(name) {
            slot.held = true;
            slot.untaken = true;
        }
        self.fail(RunFailure::NodeCouldNotStart {
            node: name.to_owned(),
            reason,
            cause: None,
        });
    }
}
