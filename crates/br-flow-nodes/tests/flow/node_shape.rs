use std::sync::Arc;

use br_flow_nodes::{LateInput, MapReduce, MapReduceError, Merger};
use br_graph_contract::{Cause, Edge, GraphBuilder, Node, PROGRAMMATIC, State, Target};

use crate::common::{Item, Mode, Probe, Report, both_filled, fan_out, merged};

struct Onward {
    name: String,
    source: String,
    targets: Vec<Target>,
}

impl Onward {
    fn from(source: &str, target: &str) -> Arc<dyn Edge<Report>> {
        Arc::new(Self {
            name: format!("after_{source}"),
            source: source.to_owned(),
            targets: vec![Target::from(target)],
        })
    }
}

impl Edge<Report> for Onward {
    fn name(&self) -> &str {
        &self.name
    }

    fn source(&self) -> &str {
        &self.source
    }

    fn targets(&self) -> &[Target] {
        &self.targets
    }

    fn evaluate(&self, _: &State<Report>) -> Result<Vec<Target>, Cause> {
        Ok(self.targets.clone())
    }
}

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

#[tokio::test(flavor = "current_thread")]
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

#[test]
fn a_merger_and_a_map_reduce_node_build_into_a_graph_that_shows_their_kinds_and_not_the_internal_node()
 {
    let (probe, _watch) = Probe::with_mode(|_| Mode::Answer);
    let merger: Arc<dyn Node<Report>> = Arc::new(Merger::new("join", both_filled, merged));
    let map_reduce: Arc<dyn Node<Report>> = Arc::new(fan_out(&probe, 2));

    let graph = GraphBuilder::new("1")
        .node(merger)
        .node(map_reduce)
        .entry("join")
        .edge(Onward::from("join", "fan"))
        .edge(Onward::from("fan", "END"))
        .build()
        .expect("a valid graph");

    let structure = graph.structure();
    let nodes: Vec<(&str, &str, &str)> = structure
        .nodes()
        .iter()
        .map(|node| {
            (
                node.name.as_str(),
                node.kind.as_str(),
                node.subkind.as_str(),
            )
        })
        .collect();
    assert_eq!(
        nodes,
        vec![
            ("fan", PROGRAMMATIC, "map_reduce"),
            ("join", PROGRAMMATIC, "merger")
        ]
    );
}
