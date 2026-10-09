use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use super::support::*;
use crate::{NodeId, Observer, Origin, Outcome, PendingEntry, Value, channel, run};

/// Node starts and records, with the occurrence they come from.
#[derive(Default)]
struct Trace(Mutex<Vec<(String, String)>>);

impl Trace {
    fn events(&self) -> Vec<(String, String)> {
        self.0.lock().unwrap().clone()
    }

    fn origins_of(&self, event: &str) -> Vec<String> {
        self.events()
            .into_iter()
            .filter(|(name, _)| name == event)
            .map(|(_, occurrence)| occurrence)
            .collect()
    }
}

impl Observer for Trace {
    fn node_started(&self, origin: &Origin, node: &NodeId) {
        let entry = (format!("start {node}"), origin.occurrence.to_string());
        self.0.lock().unwrap().push(entry);
    }

    fn recorded(&self, origin: &Origin, _entry: &PendingEntry) {
        let entry = ("recorded".to_owned(), origin.occurrence.to_string());
        self.0.lock().unwrap().push(entry);
    }
}

async fn twenty_items(trace: Arc<Trace>) -> (crate::State, Arc<Probe>) {
    let probe = Arc::new(Probe::default());
    let graph = caller(per_item(&probe)).unwrap();
    let (_sender, mut inbox) = channel();
    let ctx = context(trace);
    let outcome = run(
        &graph,
        &config(&graph),
        start(&graph, 20),
        None,
        &ctx,
        &mut inbox,
    )
    .await;
    let Ok(Outcome::Finished { state, .. }) = outcome else {
        panic!("expected finished");
    };
    (state, probe)
}

#[tokio::test(flavor = "current_thread")]
async fn given_twenty_items_five_at_a_time_when_each_calls_a_wrapper_then_results_follow_item_order_and_stay_aligned()
 {
    let trace = Arc::new(Trace::default());
    let (state, probe) = twenty_items(trace.clone()).await;
    let names = item_names(20);
    let analyses: Vec<String> = names.iter().map(|name| expected_analysis(name)).collect();
    assert_eq!(texts(&state, "analyses"), analyses);
    let attempts: Vec<Value> = names
        .iter()
        .map(|name| Value::int(needed(number(name))))
        .collect();
    assert_eq!(state.list(&key("attempts")).unwrap(), attempts.as_slice());
    assert_eq!(state.int(&key("count")).unwrap(), 7);
    assert!(state.bool(&key("flag")).unwrap());
    assert!(probe.prepared().values().all(|calls| *calls == 1));

    let finished = trace.origins_of("recorded");
    let mut in_item_order = finished.clone();
    in_item_order.sort();
    assert_eq!(finished.len(), 20);
    assert_ne!(finished, in_item_order, "the items finished in item order");

    let mut running: BTreeSet<String> = BTreeSet::new();
    let mut peak = 0;
    for (event, occurrence) in trace.events() {
        match event.as_str() {
            "start prepare" => {
                running.insert(occurrence);
            }
            "recorded" => {
                running.remove(&occurrence);
            }
            _ => {}
        }
        peak = peak.max(running.len());
    }
    assert_eq!(peak, 5);
}

#[tokio::test(flavor = "current_thread")]
async fn given_nested_runs_when_observed_then_their_events_are_told_from_the_callers() {
    let trace = Arc::new(Trace::default());
    twenty_items(trace.clone()).await;
    assert_eq!(trace.origins_of("start each"), vec![String::new()]);
    let items: BTreeSet<String> = (0..20).map(|n| format!("each[{n}]")).collect();
    for node in ["prepare", "refine"] {
        let origins: BTreeSet<String> = trace
            .origins_of(&format!("start {node}"))
            .into_iter()
            .collect();
        assert_eq!(origins, items);
    }
    let loops: BTreeSet<String> = items.iter().map(|item| format!("{item}/refine")).collect();
    for node in ["generate", "review"] {
        let origins: BTreeSet<String> = trace
            .origins_of(&format!("start {node}"))
            .into_iter()
            .collect();
        assert_eq!(origins, loops);
    }
}
