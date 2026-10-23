use super::{EdgeError, Graph};
use crate::{Fields, State, Target};

impl<F: Fields> Graph<F> {
    pub fn evaluate_edge(&self, node: &str, state: &State<F>) -> Result<Vec<Target>, EdgeError> {
        let Some(entry) = self.entry_of(node) else {
            return Err(EdgeError::UnknownNode {
                node: node.to_owned(),
            });
        };
        let returned = entry.edge.evaluate(state).map_err(EdgeError::Failed)?;
        if returned.is_empty() {
            return Err(EdgeError::EmptyTargets {
                node: node.to_owned(),
            });
        }
        let has_end = returned.iter().any(Target::is_end);
        if has_end && returned.iter().any(|target| !target.is_end()) {
            return Err(EdgeError::EndMixedWithNodes {
                node: node.to_owned(),
            });
        }
        let mut chosen = vec![false; entry.declared.len()];
        for target in &returned {
            let Some(position) = entry.position(target) else {
                return Err(EdgeError::UndeclaredTarget {
                    node: node.to_owned(),
                    target: target.clone(),
                });
            };
            chosen[position] = true;
        }
        Ok(entry
            .declared
            .iter()
            .zip(chosen)
            .filter(|(_, picked)| *picked)
            .map(|(target, _)| target.clone())
            .collect())
    }
}
