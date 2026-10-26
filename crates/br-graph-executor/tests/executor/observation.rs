use std::collections::HashSet;
use std::time::Duration;

use br_graph_contract::Target;
use br_graph_executor::{Observation, ObservedEvent, OutcomeKind};

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::observer::*;

#[tokio::test(start_paused = true)]
async fn given_a_created_instance_when_observed_then_the_report_names_its_id_node_kind_and_subkind()
{
    let rig = rig("1")
        .entry("a")
        .node_of("a", "programmatic", "merger")
        .edge(edge("a", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.result(Change::Empty);
    let log = host.observer.wait_for_outcome().await;
    assert!(log.contains(&Observation::InstanceCreated {
        instance: a.id,
        node: "a".into(),
        kind: "programmatic".into(),
        subkind: "merger".into(),
    }));
    assert!(log.contains(&Observation::InputHandedOver {
        node: "a".into(),
        instance: a.id
    }));
}

#[tokio::test(start_paused = true)]
async fn given_each_kind_of_event_when_handled_then_it_is_reported_with_the_node_and_the_instance()
{
    for kind in ["result", "failed", "refused", "cancelled"] {
        let rig = single("a");
        let host = begin(&rig, state(&[]), &["a"], Setup::new());
        let mut a = rig.stage.instance("a").await;
        let expected = match kind {
            "result" => {
                a.result(Change::Empty);
                ObservedEvent::Result
            }
            "failed" => {
                a.failed("boom");
                ObservedEvent::Failed
            }
            "refused" => {
                a.refused("no");
                ObservedEvent::Refused
            }
            _ => {
                host.cancel.trigger();
                quiesce().await;
                a.cancelled();
                ObservedEvent::Cancelled
            }
        };
        let log = host.observer.wait_for_outcome().await;
        assert!(log.contains(&Observation::EventHandled {
            node: "a".into(),
            instance: a.id,
            event: expected
        }));
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_result_when_observed_then_the_update_follows_it_and_a_refused_update_is_not_reported()
 {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.result(set("k", 1));
    let log = host.observer.wait_for_outcome().await;
    assert!(position(&log, handled("a")) < position(&log, update_applied("a")));
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.result(Change::Refuse);
    let log = host.observer.wait_for_outcome().await;
    assert_eq!(count(&log, update_applied("a")), 0);
    assert_eq!(count(&log, edge_evaluated("a")), 0);
}

#[tokio::test(start_paused = true)]
async fn given_an_evaluated_edge_when_observed_then_its_targets_come_in_declared_order_without_repeat()
 {
    let rig = rig("1")
        .entry("x")
        .node("x")
        .node("b")
        .node("c")
        .edge(edge_returning("x", &["c", "b"], &["b", "c", "b"]))
        .edge(edge("b", &["END"]))
        .edge(edge("c", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["x"], Setup::new());
    let mut x = rig.stage.instance("x").await;
    x.result(Change::Empty);
    let mut b = rig.stage.instance("b").await;
    let mut c = rig.stage.instance("c").await;
    b.result(Change::Empty);
    c.result(Change::Empty);
    let log = host.observer.wait_for_outcome().await;
    assert!(log.contains(&Observation::EdgeEvaluated {
        node: "x".into(),
        targets: vec![Target::node("c"), Target::node("b")]
    }));
}

#[tokio::test(start_paused = true)]
async fn given_each_kind_of_outcome_when_the_run_stops_then_it_is_reported_once_and_last() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.result(Change::Empty);
    let log = host.observer.wait_for_outcome().await;
    assert_eq!(
        log.last(),
        Some(&Observation::Outcome(OutcomeKind::RunEnded))
    );
    assert_eq!(count(&log, |o| matches!(o, Observation::Outcome(_))), 1);

    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.failed("boom");
    let log = host.observer.wait_for_outcome().await;
    assert_eq!(
        log.last(),
        Some(&Observation::Outcome(OutcomeKind::RunFailed))
    );
    assert_eq!(count(&log, |o| matches!(o, Observation::Outcome(_))), 1);

    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new().grace(Duration::ZERO));
    let _a = rig.stage.instance("a").await;
    host.cancel.trigger();
    let log = host.observer.wait_for_outcome().await;
    assert_eq!(
        log.last(),
        Some(&Observation::Outcome(OutcomeKind::RunCancelled))
    );
    assert_eq!(count(&log, |o| matches!(o, Observation::Outcome(_))), 1);
}

#[tokio::test(start_paused = true)]
async fn given_an_instance_when_observed_then_the_order_is_created_then_inputs_then_its_ending_event()
 {
    let rig = rig("1")
        .entry("a")
        .entry("b")
        .node("a")
        .node("b")
        .edge(routing_on("a", &["a", "b"], |fields| {
            if fields.values.get("n").copied().unwrap_or(0) < 3 {
                vec!["a", "b"]
            } else {
                vec!["b"]
            }
        }))
        .edge(edge("b", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut b = rig.stage.instance("b").await;
    for _ in 0..3 {
        let mut a = rig.stage.instance("a").await;
        a.result(add("n", 1));
    }
    host.observer.wait_for_count(handed("b"), 4).await;
    b.result(Change::Empty);
    let log = host.observer.wait_for_outcome().await;
    let mut seen_created = HashSet::new();
    let mut seen_ended = HashSet::new();
    for observation in &log {
        match observation {
            Observation::InstanceCreated { instance, .. } => {
                assert!(seen_created.insert(*instance));
            }
            Observation::InputHandedOver { instance, .. } => {
                assert!(seen_created.contains(instance));
                assert!(!seen_ended.contains(instance));
            }
            Observation::EventHandled { instance, .. } => {
                assert!(seen_created.contains(instance));
                assert!(seen_ended.insert(*instance));
            }
            _ => {}
        }
    }
    assert_eq!(seen_created, seen_ended);
}

#[tokio::test(start_paused = true)]
async fn given_events_reaching_the_executor_in_an_order_when_handled_then_the_observer_sees_that_order()
 {
    let rig = rig("1")
        .entry("a")
        .entry("b")
        .node("a")
        .node("b")
        .edge(edge("a", &["END"]))
        .edge(edge("b", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    a.result(Change::Empty);
    host.observer.wait_for(handled("a")).await;
    b.result(Change::Empty);
    let log = host.observer.wait_for_outcome().await;
    assert!(position(&log, handled("a")) < position(&log, handled("b")));
}

#[tokio::test(start_paused = true)]
async fn given_a_sink_and_an_observer_when_steps_run_then_both_see_the_same_order() {
    let rig = rig("1")
        .entry("a")
        .entry("b")
        .node("a")
        .node("a2")
        .node("b")
        .node("b2")
        .edge(edge("a", &["a2"]))
        .edge(edge("a2", &["END"]))
        .edge(edge("b", &["b2"]))
        .edge(edge("b2", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    a.result(set("a", 1));
    host.observer.wait_for(edge_evaluated("a")).await;
    b.result(set("b", 1));
    host.observer.wait_for(edge_evaluated("b")).await;
    let mut a2 = rig.stage.instance("a2").await;
    let mut b2 = rig.stage.instance("b2").await;
    a2.result(Change::Empty);
    b2.result(Change::Empty);
    let log = host.observer.wait_for_outcome().await;
    let received = sink.received();
    assert_eq!(state_of(&received[0]), kv(&[("a", 1)]));
    assert_eq!(state_of(&received[1]), kv(&[("a", 1), ("b", 1)]));
    assert!(position(&log, handled("a")) < position(&log, handled("b")));
}

#[tokio::test(start_paused = true)]
async fn given_the_observations_when_a_host_folds_them_then_it_knows_the_live_instances() {
    let rig = fan("x", &["b", "c"]);
    let host = begin(&rig, state(&[]), &["x"], Setup::new());
    let mut x = rig.stage.instance("x").await;
    x.result(Change::Empty);
    let mut b = rig.stage.instance("b").await;
    let mut c = rig.stage.instance("c").await;
    host.observer.wait_for_count(created("b"), 1).await;
    host.observer.wait_for_count(created("c"), 1).await;
    b.result(Change::Empty);
    host.observer.wait_for(handled("b")).await;
    let mut live = HashSet::new();
    for observation in host.observer.log() {
        match observation {
            Observation::InstanceCreated { instance, .. } => {
                live.insert(instance);
            }
            Observation::EventHandled { instance, .. } => {
                live.remove(&instance);
            }
            _ => {}
        }
    }
    assert_eq!(live, HashSet::from([c.id]));
    c.result(Change::Empty);
    ended(host.outcome().await);
}
