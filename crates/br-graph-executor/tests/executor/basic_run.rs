use std::time::Duration;

use br_graph_contract::{InstanceId, Target};
use br_graph_executor::Observation;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::nodes::*;
use crate::observer::*;

#[tokio::test(start_paused = true)]
async fn a_result_gives_an_input_to_each_target_with_the_state_after_the_update() {
    let rig = fan("x", &["b", "c"]);
    let host = begin(&rig, state(&[]), &["x"], Setup::new());
    let mut x = rig.stage.instance("x").await;
    x.result(set("k", 7));
    let mut b = rig.stage.instance("b").await;
    let mut c = rig.stage.instance("c").await;
    assert_eq!(b.next_input().await, kv(&[("k", 7)]));
    assert_eq!(c.next_input().await, kv(&[("k", 7)]));
    b.result(Change::Empty);
    c.result(Change::Empty);
    assert_eq!(ended(host.outcome().await), kv(&[("k", 7)]));
}

#[tokio::test(start_paused = true)]
async fn the_edge_routes_on_the_state_after_the_update() {
    let rig = rig("1")
        .entry("a")
        .node("a")
        .node("big")
        .node("small")
        .edge(routing_on("a", &["big", "small"], |fields| {
            if fields.values.get("k") == Some(&5) {
                vec!["big"]
            } else {
                vec!["small"]
            }
        }))
        .edge(edge("big", &["END"]))
        .edge(edge("small", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.result(set("k", 5));
    let mut big = rig.stage.instance("big").await;
    big.result(Change::Empty);
    ended(host.outcome().await);
    assert_eq!(rig.stage.create_calls("small"), 0);
}

#[tokio::test(start_paused = true)]
async fn an_instance_id_is_a_uuid_v7_shared_by_the_node_and_the_observer() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    assert_eq!(InstanceId::new(a.id.as_uuid()).unwrap(), a.id);
    assert_eq!(a.id.as_uuid().get_version_num(), 7);
    a.result(Change::Empty);
    host.observer.wait_for_outcome().await;
    let log = host.observer.log();
    assert!(log.contains(&Observation::InstanceCreated {
        instance: a.id,
        node: "a".into(),
        kind: "programmatic".into(),
        subkind: "worker".into(),
    }));
    assert_eq!(rig.stage.ids_of("a"), vec![a.id]);
}

#[tokio::test(start_paused = true)]
async fn a_node_is_not_idle_while_its_instance_is_being_created() {
    let rig = single("a");
    let gate = Gate::default();
    rig.stage.script("a", Creation::Gated(gate.clone()));
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    settle().await;
    assert!(!host.is_finished());
    gate.release();
    let mut a = rig.stage.instance("a").await;
    a.result(Change::Empty);
    ended(host.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn a_loop_creates_a_new_instance_with_a_new_id_each_time() {
    let rig = rig("1")
        .entry("a")
        .node("a")
        .edge(routing_on("a", &["a", "END"], |fields| {
            if fields.values.get("n").copied().unwrap_or(0) < 3 {
                vec!["a"]
            } else {
                vec!["END"]
            }
        }))
        .build();
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    for _ in 0..3 {
        let mut a = rig.stage.instance("a").await;
        assert_eq!(a.accepted_inputs().len(), 1);
        a.result(add("n", 1));
    }
    assert_eq!(value_of(&ended(host.outcome().await), "n"), Some(3));
    let ids = rig.stage.ids_of("a");
    assert_eq!(ids.len(), 3);
    for (position, id) in ids.iter().enumerate() {
        assert!(!ids[..position].contains(id));
    }
}

#[tokio::test(start_paused = true)]
async fn a_run_is_not_limited_in_its_number_of_events() {
    let rig = rig("1")
        .entry("a")
        .node("a")
        .edge(routing_on("a", &["a", "END"], |fields| {
            if fields.values.get("n").copied().unwrap_or(0) < 10_000 {
                vec!["a"]
            } else {
                vec!["END"]
            }
        }))
        .build();
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    for _ in 0..10_000 {
        let mut a = rig.stage.instance("a").await;
        a.result(add("n", 1));
    }
    assert_eq!(value_of(&ended(host.outcome().await), "n"), Some(10_000));
}

#[tokio::test(start_paused = true)]
async fn an_instance_that_accepted_three_inputs_answers_all_with_one_result() {
    let rig = rig("1")
        .entry("a")
        .entry("p")
        .entry("q")
        .node("a")
        .node("p")
        .node("q")
        .edge(edge("a", &["END"]))
        .edge(edge("p", &["a"]))
        .edge(edge("q", &["a"]))
        .build();
    let host = begin(&rig, state(&[]), &["a", "p", "q"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut p = rig.stage.instance("p").await;
    let mut q = rig.stage.instance("q").await;
    p.result(set("p", 1));
    q.result(set("q", 1));
    host.observer.wait_for_count(handed("a"), 3).await;
    assert_eq!(a.accepted_inputs().len(), 3);
    a.result(Change::Empty);
    let observer = host.observer.clone();
    ended(host.outcome().await);
    let log = observer.log();
    assert_eq!(count(&log, edge_evaluated("a")), 1);
    assert_eq!(rig.stage.create_calls("a"), 1);
}

#[tokio::test(start_paused = true)]
async fn no_node_waits_for_an_unrelated_node() {
    let rig = fan("x", &["b", "c"]);
    let host = begin(&rig, state(&[]), &["x"], Setup::new());
    let mut x = rig.stage.instance("x").await;
    x.result(Change::Empty);
    let mut b = rig.stage.instance("b").await;
    let mut c = rig.stage.instance("c").await;
    b.result(set("b", 1));
    host.observer.wait_for(edge_evaluated("b")).await;
    assert!(!host.observer.log().iter().any(handled("c")));
    c.result(set("c", 1));
    assert_eq!(ended(host.outcome().await), kv(&[("b", 1), ("c", 1)]));
}

#[tokio::test(start_paused = true)]
async fn an_expected_error_written_in_the_fields_is_normal_data() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.result(set("error_code", 404));
    assert_eq!(value_of(&ended(host.outcome().await), "error_code"), Some(404));
}

#[tokio::test(start_paused = true)]
async fn instances_are_created_just_in_time_and_never_in_advance() {
    let rig = rig("1")
        .entry("a")
        .entry("c")
        .node("a")
        .node("b")
        .node("c")
        .edge(edge("a", &["b"]))
        .edge(edge("b", &["END"]))
        .edge(edge("c", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    settle().await;
    assert_eq!(rig.stage.create_calls("b"), 0);
    a.result(Change::Empty);
    let mut b = rig.stage.instance("b").await;
    b.result(Change::Empty);
    ended(host.outcome().await);
    assert_eq!(rig.stage.create_calls("c"), 0);
}

#[tokio::test(start_paused = true)]
async fn the_final_state_holds_every_applied_update_in_order() {
    let rig = chain("1", &["a", "b", "c"]);
    let host = begin(&rig, state(&[("n", 1)]), &["a"], Setup::new());
    for (name, update) in [("a", add("n", 10)), ("b", set("m", 5)), ("c", add("n", 100))] {
        let mut instance = rig.stage.instance(name).await;
        instance.result(update);
    }
    assert_eq!(ended(host.outcome().await), kv(&[("n", 111), ("m", 5)]));
}

#[tokio::test(start_paused = true)]
async fn a_silent_instance_keeps_the_run_alive_with_no_failure_and_no_timeout() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let _a = rig.stage.instance("a").await;
    tokio::time::advance(Duration::from_secs(24 * 3600)).await;
    settle().await;
    assert!(!host.is_finished());
    assert!(!host
        .observer
        .log()
        .iter()
        .any(|o| matches!(o, Observation::Outcome(_))));
}

#[tokio::test(start_paused = true)]
async fn the_executor_retries_nothing() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.failed("boom");
    failed(host.outcome().await);
    assert_eq!(rig.stage.create_calls("a"), 1);
}

#[tokio::test(start_paused = true)]
async fn the_edge_of_a_target_end_adds_nothing() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.result(Change::Empty);
    let observer = host.observer.clone();
    ended(host.outcome().await);
    let log = observer.log();
    assert!(log.contains(&Observation::EdgeEvaluated {
        node: "a".into(),
        targets: vec![Target::End]
    }));
    assert_eq!(count(&log, created("a")), 1);
}
