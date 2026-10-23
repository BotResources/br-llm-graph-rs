use std::collections::HashSet;

use super::BuildError;
use super::names;
use super::reach;
use super::shape::Shape;
use crate::Target;

pub(super) fn validate(shape: &Shape) -> Result<(), BuildError> {
    if !names::is_version(&shape.version) {
        return Err(BuildError::InvalidVersion {
            version: shape.version.clone(),
        });
    }
    let node_names = check_nodes(shape)?;
    check_entries(shape, &node_names)?;
    check_edges(shape, &node_names)?;
    reach::check(shape)
}

fn check_nodes(shape: &Shape) -> Result<HashSet<&str>, BuildError> {
    if shape.nodes.is_empty() {
        return Err(BuildError::NoNodes);
    }
    let mut seen = HashSet::new();
    for node in &shape.nodes {
        if !names::is_node_name(&node.name) {
            return Err(BuildError::InvalidNodeName {
                name: node.name.clone(),
            });
        }
        if !names::is_node_kind(&node.kind) {
            return Err(BuildError::InvalidNodeKind {
                node: node.name.clone(),
                kind: node.kind.clone(),
            });
        }
        if !names::is_node_subkind(&node.subkind) {
            return Err(BuildError::InvalidNodeSubkind {
                node: node.name.clone(),
                subkind: node.subkind.clone(),
            });
        }
        if !seen.insert(node.name.as_str()) {
            return Err(BuildError::DuplicateNodeName {
                name: node.name.clone(),
            });
        }
    }
    Ok(seen)
}

fn check_entries(shape: &Shape, node_names: &HashSet<&str>) -> Result<(), BuildError> {
    if shape.entries.is_empty() {
        return Err(BuildError::NoEntry);
    }
    let mut seen = HashSet::new();
    for entry in &shape.entries {
        if !seen.insert(entry.as_str()) {
            return Err(BuildError::DuplicateEntry {
                name: entry.clone(),
            });
        }
        if !node_names.contains(entry.as_str()) {
            return Err(BuildError::EntryNotANode {
                name: entry.clone(),
            });
        }
    }
    Ok(())
}

fn check_edges(shape: &Shape, node_names: &HashSet<&str>) -> Result<(), BuildError> {
    let mut edge_names = HashSet::new();
    let mut sources = HashSet::new();
    for edge in &shape.edges {
        if !names::is_edge_name(&edge.name) {
            return Err(BuildError::InvalidEdgeName {
                name: edge.name.clone(),
            });
        }
        if !edge_names.insert(edge.name.as_str()) {
            return Err(BuildError::DuplicateEdgeName {
                name: edge.name.clone(),
            });
        }
        if !node_names.contains(edge.source.as_str()) {
            return Err(BuildError::EdgeSourceNotANode {
                edge: edge.name.clone(),
                source_node: edge.source.clone(),
            });
        }
        if !sources.insert(edge.source.as_str()) {
            return Err(BuildError::NodeWithSeveralEdges {
                node: edge.source.clone(),
            });
        }
        check_targets(&edge.name, &edge.targets, node_names)?;
    }
    match shape
        .nodes
        .iter()
        .find(|node| !sources.contains(node.name.as_str()))
    {
        Some(node) => Err(BuildError::NodeWithoutEdge {
            node: node.name.clone(),
        }),
        None => Ok(()),
    }
}

fn check_targets(
    edge: &str,
    targets: &[Target],
    node_names: &HashSet<&str>,
) -> Result<(), BuildError> {
    if targets.is_empty() {
        return Err(BuildError::EmptyTargets {
            edge: edge.to_owned(),
        });
    }
    let mut seen = HashSet::new();
    for target in targets {
        if !seen.insert(target) {
            return Err(BuildError::DuplicateTarget {
                edge: edge.to_owned(),
                target: target.clone(),
            });
        }
        if let Target::Node(name) = target
            && !node_names.contains(name.as_str())
        {
            return Err(BuildError::UnknownTarget {
                edge: edge.to_owned(),
                target: target.clone(),
            });
        }
    }
    Ok(())
}
