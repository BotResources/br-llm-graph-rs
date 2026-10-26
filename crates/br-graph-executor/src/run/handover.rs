use std::sync::Arc;

use br_graph_contract::{Fields, State};

use super::Run;
use super::instances::{mint_id, spawn_creation};
use super::slot::Presence;
use crate::{Observation, RunFailure};

impl<F: Fields> Run<F> {
    pub(super) fn hand_over(&mut self, targets: &[String]) {
        let mut order: Vec<String> = targets.to_vec();
        for (name, slot) in &self.slots {
            if slot.is_held() && !order.contains(name) {
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
        let Some(slot) = self.slots.get(name) else {
            return;
        };
        if !slot.is_held() {
            return;
        }
        match slot.presence() {
            Presence::Creating => {}
            Presence::Running => self.give_held(name),
            Presence::Absent => self.begin_creation(name),
        }
    }

    pub(super) fn give_held(&mut self, name: &str) {
        let state = Arc::clone(&self.state);
        if self.give_input(name, state) {
            self.release(name);
        }
    }

    fn release(&mut self, name: &str) {
        if let Some(slot) = self.slots.get_mut(name) {
            slot.release();
        }
    }

    pub(super) fn give_input(&self, name: &str, state: Arc<State<F>>) -> bool {
        let Some(instance) = self.slots.get(name).and_then(|slot| slot.give(state)) else {
            return false;
        };
        self.observing.tell(|| Observation::InputHandedOver {
            node: name.to_owned(),
            instance,
        });
        true
    }

    fn begin_creation(&mut self, name: &str) {
        let Some(slot) = self.slots.get(name) else {
            return;
        };
        let node = slot.node();
        let first_input = Arc::clone(&self.state);
        let id = match mint_id() {
            Ok(id) => id,
            Err(reason) => {
                self.creation_impossible(name, reason);
                return;
            }
        };
        let creation = spawn_creation(
            node,
            name.to_owned(),
            self.cancel.view(),
            id,
            self.sender.clone(),
        );
        if let Some(slot) = self.slots.get_mut(name) {
            slot.release();
            slot.begin_creating(id, first_input, creation);
        }
    }

    fn creation_impossible(&mut self, name: &str, reason: String) {
        let failure = RunFailure::NodeCouldNotStart {
            node: name.to_owned(),
            reason,
            cause: None,
        };
        self.fail_node(name, failure);
    }
}
