use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_flow_nodes::MapReduce;
use br_graph_contract::State;

use super::{Item, Probe, Report, ReportUpdate};

pub fn nz(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("a positive number")
}

pub fn items_of(state: &State<Report>) -> Vec<Item> {
    state.fields().items.iter().copied().map(Item::of).collect()
}

pub fn summed(states: &[State<Item>]) -> ReportUpdate {
    ReportUpdate::Reduced {
        sum: states.iter().filter_map(|state| state.fields().out).sum(),
        errors: states
            .iter()
            .filter_map(|state| state.fields().error.clone())
            .collect(),
    }
}

pub fn in_order(states: &[State<Item>]) -> ReportUpdate {
    ReportUpdate::Reduced {
        sum: 0,
        errors: states
            .iter()
            .map(|state| state.fields().value.to_string())
            .collect(),
    }
}

pub fn fan_out(probe: &Arc<Probe>, limit: usize) -> MapReduce<Report, Item> {
    MapReduce::new("fan", items_of, probe.clone(), nz(limit), summed)
}

pub fn counting_fan_out(
    probe: &Arc<Probe>,
    limit: usize,
) -> (MapReduce<Report, Item>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let maps = Arc::new(AtomicUsize::new(0));
    let reduces = Arc::new(AtomicUsize::new(0));
    let map_counter = Arc::clone(&maps);
    let reduce_counter = Arc::clone(&reduces);
    let node = MapReduce::new(
        "fan",
        move |state: &State<Report>| {
            map_counter.fetch_add(1, Ordering::SeqCst);
            items_of(state)
        },
        probe.clone(),
        nz(limit),
        move |states: &[State<Item>]| {
            reduce_counter.fetch_add(1, Ordering::SeqCst);
            summed(states)
        },
    );
    (node, maps, reduces)
}
