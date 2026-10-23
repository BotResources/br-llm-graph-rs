use std::collections::HashMap;

use super::BuildError;
use super::shape::Shape;
use crate::Target;

pub(super) fn check(shape: &Shape) -> Result<(), BuildError> {
    let index: HashMap<&str, usize> = shape
        .nodes
        .iter()
        .enumerate()
        .map(|(position, node)| (node.name.as_str(), position))
        .collect();
    let mut forward = vec![Vec::new(); shape.nodes.len()];
    let mut backward = vec![Vec::new(); shape.nodes.len()];
    let mut exits = Vec::new();
    for edge in &shape.edges {
        let Some(&from) = index.get(edge.source.as_str()) else {
            continue;
        };
        for target in &edge.targets {
            match target {
                Target::End => exits.push(from),
                Target::Node(name) => {
                    if let Some(&to) = index.get(name.as_str()) {
                        forward[from].push(to);
                        backward[to].push(from);
                    }
                }
            }
        }
    }
    let entries = shape
        .entries
        .iter()
        .filter_map(|entry| index.get(entry.as_str()).copied());
    let reached = flood(&forward, entries);
    if let Some(position) = reached.iter().position(|seen| !seen) {
        return Err(BuildError::UnreachableNode {
            node: shape.nodes[position].name.clone(),
        });
    }
    let exiting = flood(&backward, exits);
    match exiting.iter().position(|seen| !seen) {
        Some(position) => Err(BuildError::EndUnreachable {
            node: shape.nodes[position].name.clone(),
        }),
        None => Ok(()),
    }
}

fn flood(adjacency: &[Vec<usize>], seeds: impl IntoIterator<Item = usize>) -> Vec<bool> {
    let mut seen = vec![false; adjacency.len()];
    let mut pending: Vec<usize> = Vec::new();
    for seed in seeds {
        if !seen[seed] {
            seen[seed] = true;
            pending.push(seed);
        }
    }
    while let Some(current) = pending.pop() {
        for &next in &adjacency[current] {
            if !seen[next] {
                seen[next] = true;
                pending.push(next);
            }
        }
    }
    seen
}
