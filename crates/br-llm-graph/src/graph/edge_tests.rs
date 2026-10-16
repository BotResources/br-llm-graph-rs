use super::*;
use crate::state::{Config, State};
use crate::value::{EndLabel, NodeId};

fn to(name: &str) -> Target {
    Target::Node(NodeId::new(name).unwrap())
}

fn end(label: &str) -> Target {
    Target::End(EndLabel::new(label).unwrap())
}

/// A host's edge that does not say what it declares.
struct HostEdge;

impl Edge for HostEdge {
    fn next(&self, _state: &State, _config: &Config) -> Result<Vec<Target>, GraphError> {
        Ok(vec![end("done")])
    }
}

#[test]
fn given_always_when_asked_its_targets_then_it_declares_its_one_target() {
    assert_eq!(Always(to("next")).declared(), Some(&[to("next")][..]));
    assert_eq!(Always(end("done")).declared(), Some(&[end("done")][..]));
}

#[test]
fn given_an_edge_built_from_a_function_when_not_given_targets_then_undeclared() {
    let edge = FnEdge::new(|_: &State, _: &Config| Ok(vec![end("done")]));
    assert_eq!(edge.declared(), None);
}

#[test]
fn given_a_host_edge_without_declared_when_asked_then_undeclared_by_default() {
    assert_eq!(HostEdge.declared(), None);
}

#[test]
fn given_targets_when_declared_then_kept_as_given() {
    let edge = FnEdge::new(|_: &State, _: &Config| Ok(vec![end("done")])).targets([
        to("b"),
        end("done"),
        to("a"),
        to("b"),
    ]);
    assert_eq!(
        edge.declared(),
        Some(&[to("b"), end("done"), to("a"), to("b")][..])
    );
}

#[test]
fn given_no_target_when_declared_then_the_edge_declares_an_empty_list() {
    let edge = FnEdge::new(|_: &State, _: &Config| Ok(vec![end("done")])).targets([]);
    assert_eq!(edge.declared(), Some(&[][..]));
}

#[test]
fn given_a_target_when_displayed_then_it_names_a_node_or_an_end() {
    assert_eq!(to("next").to_string(), "node next");
    assert_eq!(end("done").to_string(), "end done");
}
