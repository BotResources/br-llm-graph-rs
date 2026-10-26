use br_graph_contract::State;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;

#[tokio::test(start_paused = true)]
async fn the_state_of_a_checkpoint_is_the_exact_serialization_of_the_state_and_the_structure_the_graph_s()
 {
    let rig = single("a");
    let host = begin(&rig, state(&[("k", 1)]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.failed("boom");
    let (_, checkpoint) = failed(host.outcome().await);
    let expected = state(&[("k", 1)]).to_json().unwrap();
    assert_eq!(checkpoint.state_json().as_bytes(), expected.as_slice());
    assert_eq!(checkpoint.structure(), &rig.graph.structure());
}

#[tokio::test(start_paused = true)]
async fn a_host_can_start_again_from_the_state_and_the_node_names_of_a_checkpoint() {
    let rig = fan("x", &["b", "c"]);
    let host = begin(&rig, state(&[("k", 5)]), &["c", "b"], Setup::new());
    let mut b = rig.stage.instance("b").await;
    let _c = rig.stage.instance("c").await;
    b.failed("boom");
    let (_, checkpoint) = failed(host.outcome().await);
    assert_eq!(checkpoint.nodes(), ["b", "c"]);
    let rebuilt = State::<Kv>::rebuild(checkpoint.state_json().as_bytes()).unwrap();
    let names: Vec<&str> = checkpoint.nodes().iter().map(String::as_str).collect();
    let again = begin(&rig, rebuilt, &names, Setup::new());
    let mut b_again = rig.stage.instance("b").await;
    let mut c_again = rig.stage.instance("c").await;
    assert_eq!(b_again.accepted_inputs(), vec![kv(&[("k", 5)])]);
    b_again.result(Change::Empty);
    c_again.result(Change::Empty);
    ended(again.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn a_resumed_state_keeps_the_limit_the_host_gave() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.failed("boom");
    let (_, checkpoint) = failed(host.outcome().await);
    let limit = std::num::NonZeroUsize::new(100).unwrap();
    let resumed = begin_resume(&rig, checkpoint, limit, Setup::new());
    let mut again = rig.stage.instance("a").await;
    again.result(Change::Grow(200));
    let (failures, _) = failed(resumed.outcome().await);
    assert!(matches!(
        failures[0],
        br_graph_executor::RunFailure::UpdateRefused { .. }
    ));
}

#[tokio::test(start_paused = true)]
async fn every_node_a_checkpoint_names_gets_a_new_instance_with_one_input_of_its_state() {
    let rig = fan("x", &["b", "c"]);
    let host = begin(&rig, state(&[("k", 5)]), &["b", "c"], Setup::new());
    let mut b = rig.stage.instance("b").await;
    let c = rig.stage.instance("c").await;
    b.failed("boom");
    let (_, checkpoint) = failed(host.outcome().await);
    let resumed = begin_resume(&rig, checkpoint, BIG, Setup::new());
    let mut b_again = rig.stage.instance("b").await;
    let mut c_again = rig.stage.instance("c").await;
    assert_ne!(b_again.id, b.id);
    assert_ne!(c_again.id, c.id);
    assert_eq!(b_again.accepted_inputs(), vec![kv(&[("k", 5)])]);
    assert_eq!(c_again.accepted_inputs(), vec![kv(&[("k", 5)])]);
    b_again.result(Change::Empty);
    c_again.result(Change::Empty);
    ended(resumed.outcome().await);
}

#[test]
fn every_failure_names_its_cause_in_its_display() {
    use br_graph_executor::RunFailure;
    let failures = [
        RunFailure::NodeFailed {
            node: "n".into(),
            reason: "why".into(),
            cause: "c".into(),
        },
        RunFailure::NodeRefusedInput {
            node: "n".into(),
            reason: "why".into(),
        },
        RunFailure::NodeCouldNotStart {
            node: "n".into(),
            reason: "why".into(),
            cause: None,
        },
        RunFailure::NodeVanished {
            node: "n".into(),
            reason: "why".into(),
        },
        RunFailure::UpdateRefused {
            node: "n".into(),
            reason: "why".into(),
        },
        RunFailure::EdgeFailed {
            node: "n".into(),
            reason: "why".into(),
            cause: None,
        },
        RunFailure::SinkRefused {
            reason: "why".into(),
            checkpoint: handwritten_checkpoint(&single("n"), "{}", &["n"]),
        },
        RunFailure::CheckpointNotBuilt {
            reason: "why".into(),
        },
    ];
    for failure in failures {
        let text = failure.to_string();
        assert!(text.contains("why"));
    }
}
