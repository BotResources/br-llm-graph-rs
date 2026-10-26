use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use br_graph_contract::{EdgeError, Fields, Target};

use super::Run;
use crate::panic_text::{display_of, panic_reason};
use crate::{Observation, RunFailure};

impl<F: Fields> Run<F> {
    pub(super) async fn result_step(&mut self, node: String, update: F::Update) {
        let current = Arc::clone(&self.state);
        let new_state = match catch_unwind(AssertUnwindSafe(|| current.apply(update))) {
            Ok(Ok(state)) => state,
            Ok(Err(error)) => {
                self.undo(&node, update_refused(&node, error.to_string()));
                return;
            }
            Err(payload) => {
                self.undo(&node, update_refused(&node, panic_reason(&*payload)));
                return;
            }
        };
        let evaluated = catch_unwind(AssertUnwindSafe(|| {
            self.graph.evaluate_edge(&node, &new_state)
        }));
        let targets = match evaluated {
            Ok(Ok(targets)) => targets,
            Ok(Err(EdgeError::Failed(cause))) => {
                let failure = RunFailure::EdgeFailed {
                    node: node.clone(),
                    reason: display_of(&*cause),
                    cause: Some(cause),
                };
                self.undo(&node, failure);
                return;
            }
            Ok(Err(other)) => {
                self.undo(&node, edge_failed(&node, other.to_string()));
                return;
            }
            Err(payload) => {
                self.undo(&node, edge_failed(&node, panic_reason(&*payload)));
                return;
            }
        };
        self.state = Arc::new(new_state);
        self.observing
            .tell(|| Observation::UpdateApplied { node: node.clone() });
        self.observing.tell(|| Observation::EdgeEvaluated {
            node: node.clone(),
            targets: targets.clone(),
        });
        let mut named = Vec::new();
        for target in &targets {
            if let Target::Node(name) = target {
                if let Some(slot) = self.slots.get_mut(name) {
                    slot.held = true;
                }
                named.push(name.clone());
            }
        }
        if self.all_idle() {
            return;
        }
        if !self.is_stopping() {
            self.offer_checkpoint_to_sink().await;
            self.read_token();
        }
        if !self.is_stopping() {
            self.hand_over(&named);
        }
    }

    fn undo(&mut self, node: &str, failure: RunFailure) {
        if let Some(slot) = self.slots.get_mut(node) {
            slot.untaken = true;
        }
        self.fail(failure);
    }
}

fn update_refused(node: &str, reason: String) -> RunFailure {
    RunFailure::UpdateRefused {
        node: node.to_owned(),
        reason,
    }
}

fn edge_failed(node: &str, reason: String) -> RunFailure {
    RunFailure::EdgeFailed {
        node: node.to_owned(),
        reason,
        cause: None,
    }
}
