use br_graph_contract::State;
use br_graph_executor::Checkpoint;
use serde_json::json;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::resume_support::*;

#[tokio::test(start_paused = true)]
async fn given_each_resume_refusal_when_displayed_then_the_text_names_its_reason_exactly() {
    let rig = base();
    let checkpoint = checkpoint_of(&rig).await;
    let exact = [
        (
            refusal(&chain("2", &["a", "b"]), checkpoint.clone()).await,
            "checkpoint version \"1\" differs from graph version \"2\"",
        ),
        (
            refusal(&chain("1", &["a"]), checkpoint.clone()).await,
            "checkpoint structure differs from the graph: the node names differ",
        ),
        (
            refusal(&rig, forged(&checkpoint, |v| v["nodes"] = json!([]))).await,
            "checkpoint names no node",
        ),
        (
            refusal(&rig, forged(&checkpoint, |v| v["nodes"] = json!(["zz"]))).await,
            "checkpoint names node \"zz\", which the graph does not hold",
        ),
        (
            refusal(
                &rig,
                forged(&checkpoint, |v| v["nodes"] = json!(["a", "a"])),
            )
            .await,
            "checkpoint names node \"a\" twice",
        ),
    ];
    for (error, text) in exact {
        assert_eq!(error.to_string(), text);
    }
    let rejected = refusal(&rig, forged(&checkpoint, |v| v["state"] = json!("1"))).await;
    assert!(
        rejected
            .to_string()
            .starts_with("checkpoint state cannot be rebuilt: ")
    );
    let malformed = Checkpoint::from_json_slice(b"nope").unwrap_err();
    assert!(
        malformed
            .to_string()
            .starts_with("checkpoint cannot be rebuilt: ")
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_serialized_checkpoint_when_resumed_then_the_run_goes_on() {
    let rig = base();
    let checkpoint = checkpoint_of(&rig).await;
    let bytes = serde_json::to_vec(&checkpoint).unwrap();
    let rebuilt = Checkpoint::from_json_slice(&bytes).unwrap();
    assert_eq!(rebuilt, checkpoint);
    let host = begin_resume(&rig, rebuilt, BIG, Setup::new());
    let mut a = rig.stage.instance("a").await;
    assert_eq!(a.next_input().await, kv(&[("k", 1)]));
    a.failed("again");
    failed(host.outcome().await);
    let _ = State::<Kv>::rebuild(checkpoint.state_json().as_bytes()).unwrap();
}
