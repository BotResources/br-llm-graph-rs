use std::sync::Arc;

use br_flow_nodes::{LateInput, MapReduce, MapReduceError, Merger};
use br_graph_contract::{Node, PROGRAMMATIC};

use crate::common::{Item, Mode, Probe, Report, both_filled, fan_out, merged};

fn is_send_sync_static<T: Send + Sync + 'static>() {}

#[test]
fn the_merger_is_a_programmatic_node_with_the_subkind_merger_and_the_given_name() {
    let node: Merger<Report> = Merger::new("join_reports", both_filled, merged);

    assert_eq!(node.name(), "join_reports");
    assert_eq!(node.kind(), PROGRAMMATIC);
    assert_eq!(node.subkind(), "merger");
}

#[test]
fn the_late_input_policy_does_not_change_the_kind_or_the_subkind() {
    let node: Merger<Report> = Merger::new("join", both_filled, merged).late_input(LateInput::Fail);

    assert_eq!(node.kind(), PROGRAMMATIC);
    assert_eq!(node.subkind(), "merger");
}

#[tokio::test]
async fn the_map_reduce_is_a_programmatic_node_with_the_subkind_map_reduce_and_the_given_name() {
    let (probe, _watch) = Probe::with_mode(|_| Mode::Answer);
    let node: MapReduce<Report, Item> = fan_out(&probe, 2);

    assert_eq!(node.name(), "fan");
    assert_eq!(node.kind(), PROGRAMMATIC);
    assert_eq!(node.subkind(), "map_reduce");
}

#[test]
fn the_nodes_and_the_error_can_be_shared_and_sent_between_threads() {
    is_send_sync_static::<Merger<Report>>();
    is_send_sync_static::<MapReduce<Report, Item>>();
    is_send_sync_static::<MapReduceError>();
    is_send_sync_static::<Arc<dyn Node<Report>>>();
}
