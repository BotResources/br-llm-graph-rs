use br_graph_contract::State;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;

#[tokio::test(start_paused = true)]
async fn given_a_failed_run_when_its_checkpoint_is_read_then_it_holds_the_state_and_the_structure()
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
async fn given_a_checkpoint_when_the_host_reads_its_state_and_nodes_then_it_can_start_a_new_run_from_them()
 {
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
async fn given_a_resume_limit_when_an_update_exceeds_it_then_the_update_is_refused() {
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
async fn given_a_checkpoint_naming_nodes_when_resumed_then_each_gets_a_new_instance_and_one_input()
{
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
fn given_each_kind_of_failure_when_displayed_then_the_text_names_the_node_and_the_reason() {
    use br_graph_executor::RunFailure;
    let failures = [
        (
            RunFailure::NodeFailed {
                node: "n".into(),
                reason: "why".into(),
                cause: "c".into(),
            },
            "node \"n\" failed: why",
        ),
        (
            RunFailure::NodeRefusedInput {
                node: "n".into(),
                reason: "why".into(),
            },
            "node \"n\" refused its input: why",
        ),
        (
            RunFailure::NodeCouldNotStart {
                node: "n".into(),
                reason: "why".into(),
                cause: None,
            },
            "node \"n\" could not start: why",
        ),
        (
            RunFailure::NodeVanished {
                node: "n".into(),
                reason: "why".into(),
            },
            "node \"n\" vanished: why",
        ),
        (
            RunFailure::UpdateRefused {
                node: "n".into(),
                reason: "why".into(),
            },
            "update of node \"n\" refused: why",
        ),
        (
            RunFailure::EdgeFailed {
                node: "n".into(),
                reason: "why".into(),
                cause: None,
            },
            "edge of node \"n\" failed: why",
        ),
        (
            RunFailure::SinkRefused {
                reason: "why".into(),
                checkpoint: handwritten_checkpoint(&single("n"), "{}", &["n"]),
            },
            "checkpoint sink refused: why",
        ),
        (
            RunFailure::CheckpointNotBuilt {
                reason: "why".into(),
            },
            "checkpoint not built: why",
        ),
    ];
    for (failure, text) in failures {
        assert_eq!(failure.to_string(), text);
    }
}
