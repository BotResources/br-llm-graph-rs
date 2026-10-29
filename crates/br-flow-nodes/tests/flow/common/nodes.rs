use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_flow_nodes::{ElementOutcome, FaultMode, MapReduce};
use br_graph_contract::State;

use super::{CreateMode, Item, Mode, Probe, Report, ReportUpdate};

pub fn nz(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).expect("a positive number")
}

pub fn items_of(state: &State<Report>) -> Vec<Item> {
    state.fields().items.iter().copied().map(Item::of).collect()
}

pub fn summed(outcomes: &[ElementOutcome<Item>]) -> ReportUpdate {
    ReportUpdate::Reduced {
        sum: outcomes
            .iter()
            .filter_map(|outcome| outcome.state()?.fields().out)
            .sum(),
        errors: outcomes
            .iter()
            .filter_map(|outcome| match outcome {
                ElementOutcome::Done(state) => state.fields().error.clone(),
                ElementOutcome::Failed(text) => Some(text.clone()),
            })
            .collect(),
    }
}

pub fn in_order(outcomes: &[ElementOutcome<Item>]) -> ReportUpdate {
    ReportUpdate::Reduced {
        sum: 0,
        errors: outcomes
            .iter()
            .map(|outcome| match outcome {
                ElementOutcome::Done(state) => {
                    format!("{}:{:?}", state.fields().value, state.fields().out)
                }
                ElementOutcome::Failed(text) => format!("failed:{text}"),
            })
            .collect(),
    }
}

pub fn ordered(probe: &Arc<Probe>, limit: usize) -> MapReduce<Report, Item> {
    MapReduce::new("fan", items_of, probe.clone(), nz(limit), in_order)
}

pub fn mode_at(fault: u32, mode: Mode) -> impl Fn(u32) -> Mode + Send + Sync + 'static {
    move |value| if value == fault { mode } else { Mode::Answer }
}

pub fn creation_at(
    position: usize,
    create: CreateMode,
) -> impl Fn(usize) -> CreateMode + Send + Sync + 'static {
    move |created| {
        if created == position {
            create
        } else {
            CreateMode::Normal
        }
    }
}

pub fn fan_out(probe: &Arc<Probe>, limit: usize) -> MapReduce<Report, Item> {
    MapReduce::new("fan", items_of, probe.clone(), nz(limit), summed)
}

pub fn strict_fan_out(probe: &Arc<Probe>, limit: usize) -> MapReduce<Report, Item> {
    fan_out(probe, limit).fault_mode(FaultMode::Strict)
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
        move |outcomes: &[ElementOutcome<Item>]| {
            reduce_counter.fetch_add(1, Ordering::SeqCst);
            summed(outcomes)
        },
    );
    (node, maps, reduces)
}

pub type Captured = Arc<Mutex<Vec<ElementOutcome<Item>>>>;

pub fn capturing_fan_out(
    probe: &Arc<Probe>,
    limit: usize,
    mode: FaultMode,
) -> (MapReduce<Report, Item>, Captured) {
    let captured: Captured = Arc::default();
    let sink = Arc::clone(&captured);
    let node = MapReduce::new(
        "fan",
        items_of,
        probe.clone(),
        nz(limit),
        move |outcomes: &[ElementOutcome<Item>]| {
            *sink.lock().unwrap() = outcomes.to_vec();
            summed(outcomes)
        },
    )
    .fault_mode(mode);
    (node, captured)
}
