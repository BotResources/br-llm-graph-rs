use br_graph_executor::RunFailure;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;

async fn failing_run(kind: &str) -> (Rig, br_graph_executor::Checkpoint) {
    let rig = match kind {
        "edge" => rig("1")
            .entry("a")
            .node("a")
            .edge(edge_failing("a", &["END"], "edge broke"))
            .build(),
        _ => single("a"),
    };
    let host = begin(&rig, state(&[("k", 1)]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    match kind {
        "update" => a.result(Change::Refuse),
        "edge" => a.result(set("k", 9)),
        _ => a.refused("not twice"),
    }
    let (_, checkpoint) = failed(host.outcome().await);
    (rig, checkpoint)
}

#[tokio::test(start_paused = true)]
async fn given_a_refused_update_an_edge_failure_or_a_refused_input_when_resumed_then_the_node_runs_again()
 {
    for kind in ["update", "edge", "refused"] {
        let (rig, checkpoint) = failing_run(kind).await;
        let host = begin_resume(&rig, checkpoint, BIG, Setup::new());
        let mut again = rig.stage.instance("a").await;
        assert_eq!(again.accepted_inputs(), vec![kv(&[("k", 1)])]);
        again.failed("stop");
        failed(host.outcome().await);
        assert_eq!(rig.stage.create_calls("a"), 2);
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_refused_input_when_the_run_is_resumed_then_the_node_refuses_it_again() {
    let (rig, checkpoint) = failing_run("refused").await;
    let host = begin_resume(&rig, checkpoint, BIG, Setup::new());
    let mut again = rig.stage.instance("a").await;
    again.refused("not twice");
    let (failures, _) = failed(host.outcome().await);
    assert!(matches!(&failures[0], RunFailure::NodeRefusedInput { .. }));
}
