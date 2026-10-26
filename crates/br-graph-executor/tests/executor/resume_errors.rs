use br_graph_contract::State;
use br_graph_executor::{Checkpoint, ResumeError, resume};
use serde_json::{Value, json};

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;

async fn checkpoint_of(rig: &Rig) -> Checkpoint {
    let host = begin(rig, state(&[("k", 1)]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.failed("stop here");
    failed(host.outcome().await).1
}

fn forged(checkpoint: &Checkpoint, edit: impl FnOnce(&mut Value)) -> Checkpoint {
    let mut value: Value =
        serde_json::from_slice(&serde_json::to_vec(checkpoint).unwrap()).unwrap();
    edit(&mut value);
    Checkpoint::from_json_slice(&serde_json::to_vec(&value).unwrap()).unwrap()
}

async fn refusal(rig: &Rig, checkpoint: Checkpoint) -> ResumeError {
    let (config, ..) = Setup::new().config();
    within(resume(rig.graph.clone(), checkpoint, BIG, config))
        .await
        .unwrap_err()
}

fn base() -> Rig {
    chain("1", &["a", "b"])
}

#[tokio::test(start_paused = true)]
async fn given_a_checkpoint_of_another_version_when_resumed_then_it_is_refused() {
    let checkpoint = checkpoint_of(&base()).await;
    let error = refusal(&chain("2", &["a", "b"]), checkpoint).await;
    assert!(matches!(
        error,
        ResumeError::VersionMismatch { checkpoint, graph } if checkpoint == "1" && graph == "2"
    ));
}

#[tokio::test(start_paused = true)]
async fn given_versions_that_differ_in_any_order_when_resumed_then_only_equality_counts() {
    for (old, new) in [("1", "2"), ("2", "1"), ("1.9", "1.10"), ("1.10", "1.9")] {
        let checkpoint = checkpoint_of(&chain(old, &["a", "b"])).await;
        let error = refusal(&chain(new, &["a", "b"]), checkpoint).await;
        assert!(matches!(error, ResumeError::VersionMismatch { .. }));
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_graph_with_more_nodes_when_resumed_then_the_checkpoint_is_refused() {
    let checkpoint = checkpoint_of(&base()).await;
    let error = refusal(&chain("1", &["a", "b", "c"]), checkpoint).await;
    assert!(matches!(error, ResumeError::StructureMismatch { .. }));
}

#[tokio::test(start_paused = true)]
async fn given_a_graph_with_fewer_nodes_when_resumed_then_the_checkpoint_is_refused() {
    let checkpoint = checkpoint_of(&base()).await;
    let error = refusal(&chain("1", &["a"]), checkpoint).await;
    assert!(matches!(error, ResumeError::StructureMismatch { .. }));
}

#[tokio::test(start_paused = true)]
async fn given_a_renamed_node_when_resumed_then_the_checkpoint_is_refused() {
    let checkpoint = checkpoint_of(&base()).await;
    let error = refusal(&chain("1", &["a", "c"]), checkpoint).await;
    assert!(matches!(error, ResumeError::StructureMismatch { .. }));
}

fn with_edges(a_targets: &[&str], a_edge_name: &str, a_source: &str, b_source: &str) -> Rig {
    rig("1")
        .entry("a")
        .node("a")
        .node("b")
        .edge(named_edge(a_edge_name, a_source, a_targets))
        .edge(named_edge("after_b", b_source, &["END"]))
        .build()
}

#[tokio::test(start_paused = true)]
async fn given_another_set_of_targets_when_resumed_then_the_checkpoint_is_refused() {
    let checkpoint = checkpoint_of(&with_edges(&["b"], "after_a", "a", "b")).await;
    let error = refusal(&with_edges(&["b", "END"], "after_a", "a", "b"), checkpoint).await;
    assert!(matches!(error, ResumeError::StructureMismatch { .. }));
}

#[tokio::test(start_paused = true)]
async fn given_the_same_targets_in_another_order_when_resumed_then_the_checkpoint_is_refused() {
    let checkpoint = checkpoint_of(&with_edges(&["b", "END"], "after_a", "a", "b")).await;
    let error = refusal(&with_edges(&["END", "b"], "after_a", "a", "b"), checkpoint).await;
    assert!(matches!(error, ResumeError::StructureMismatch { .. }));
}

#[tokio::test(start_paused = true)]
async fn given_an_edge_with_another_name_when_resumed_then_the_checkpoint_is_refused() {
    let checkpoint = checkpoint_of(&with_edges(&["b"], "after_a", "a", "b")).await;
    let error = refusal(&with_edges(&["b"], "leave_a", "a", "b"), checkpoint).await;
    assert!(matches!(error, ResumeError::StructureMismatch { .. }));
}

#[tokio::test(start_paused = true)]
async fn given_an_edge_with_another_source_when_resumed_then_the_checkpoint_is_refused() {
    let one = rig("1")
        .entry("a")
        .node("a")
        .node("b")
        .edge(named_edge("first_edge", "a", &["b"]))
        .edge(named_edge("second_edge", "b", &["END"]))
        .build();
    let other = rig("1")
        .entry("a")
        .node("a")
        .node("b")
        .edge(named_edge("first_edge", "b", &["END"]))
        .edge(named_edge("second_edge", "a", &["b"]))
        .build();
    let checkpoint = checkpoint_of(&one).await;
    let error = refusal(&other, checkpoint).await;
    assert!(matches!(error, ResumeError::StructureMismatch { .. }));
}

#[tokio::test(start_paused = true)]
async fn given_other_entries_kinds_and_subkinds_when_resumed_then_the_checkpoint_is_accepted() {
    let checkpoint = checkpoint_of(&base()).await;
    let other_entries = rig("1")
        .entry("a")
        .entry("b")
        .node("a")
        .node("b")
        .edge(edge("a", &["b"]))
        .edge(edge("b", &["END"]))
        .build();
    let other_kinds = rig("1")
        .entry("a")
        .node_of("a", "custom", "special")
        .node_of("b", "custom", "other_special")
        .edge(edge("a", &["b"]))
        .edge(edge("b", &["END"]))
        .build();
    for rig in [other_entries, other_kinds] {
        let host = begin_resume(&rig, checkpoint.clone(), BIG, Setup::new());
        let mut a = rig.stage.instance("a").await;
        a.result(set("done", 1));
        let mut b = rig.stage.instance("b").await;
        b.result(Change::Empty);
        assert_eq!(value_of(&ended(host.outcome().await), "done"), Some(1));
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_state_over_the_limit_when_resumed_then_it_is_rejected_and_an_exact_limit_is_accepted()
 {
    let rig = base();
    let checkpoint = checkpoint_of(&rig).await;
    let (config, ..) = Setup::new().config();
    let limit = std::num::NonZeroUsize::new(5).unwrap();
    let error = within(resume(rig.graph.clone(), checkpoint.clone(), limit, config))
        .await
        .unwrap_err();
    assert!(matches!(error, ResumeError::StateRejected { .. }));
    let (config, ..) = Setup::new().config();
    let exact = std::num::NonZeroUsize::new(checkpoint.state_json().len()).unwrap();
    let host_run = tokio::spawn(resume(rig.graph.clone(), checkpoint, exact, config));
    rig.stage.detach();
    let mut a = rig.stage.instance("a").await;
    a.next_input().await;
    a.failed("stop");
    assert!(within(host_run).await.unwrap().is_ok());
}

#[tokio::test(start_paused = true)]
async fn given_a_state_with_invalid_fields_or_the_wrong_shape_when_resumed_then_it_is_rejected_as_a_state()
 {
    let rig = base();
    let checkpoint = checkpoint_of(&rig).await;
    let invalid = forged(&checkpoint, |value| {
        value["state"] = json!(r#"{"values":{"invalid":1},"pad":""}"#);
    });
    assert!(matches!(
        refusal(&rig, invalid).await,
        ResumeError::StateRejected { .. }
    ));
    let wrong_shape = forged(&checkpoint, |value| value["state"] = json!("12"));
    assert!(matches!(
        refusal(&rig, wrong_shape).await,
        ResumeError::StateRejected { .. }
    ));
}

#[tokio::test(start_paused = true)]
async fn given_an_unknown_node_when_resumed_then_the_checkpoint_is_refused() {
    let rig = base();
    let checkpoint = checkpoint_of(&rig).await;
    let forged = forged(&checkpoint, |value| value["nodes"] = json!(["zz"]));
    assert!(matches!(
        refusal(&rig, forged).await,
        ResumeError::UnknownNode { name } if name == "zz"
    ));
}

#[tokio::test(start_paused = true)]
async fn given_a_node_named_twice_when_resumed_then_the_checkpoint_is_refused() {
    let rig = base();
    let checkpoint = checkpoint_of(&rig).await;
    let forged = forged(&checkpoint, |value| value["nodes"] = json!(["a", "a"]));
    assert!(matches!(
        refusal(&rig, forged).await,
        ResumeError::DuplicateNode { name } if name == "a"
    ));
}

#[tokio::test(start_paused = true)]
async fn given_a_checkpoint_naming_no_node_when_resumed_then_it_is_refused() {
    let rig = base();
    let checkpoint = checkpoint_of(&rig).await;
    let forged = forged(&checkpoint, |value| value["nodes"] = json!([]));
    assert!(matches!(refusal(&rig, forged).await, ResumeError::NoNodes));
}

#[test]
fn given_bytes_that_are_no_checkpoint_when_read_then_they_are_malformed() {
    let cases: [&[u8]; 4] = [b"{\"state\":", b"{}", b"[]", b"not json"];
    for bytes in cases {
        assert!(matches!(
            Checkpoint::from_json_slice(bytes),
            Err(ResumeError::Malformed { .. })
        ));
    }
}

#[tokio::test(start_paused = true)]
async fn given_an_unknown_field_or_a_broken_structure_when_read_then_the_checkpoint_is_malformed() {
    let checkpoint = checkpoint_of(&base()).await;
    let mut value: Value =
        serde_json::from_slice(&serde_json::to_vec(&checkpoint).unwrap()).unwrap();
    value["extra"] = json!(1);
    let unknown = serde_json::to_vec(&value).unwrap();
    assert!(matches!(
        Checkpoint::from_json_slice(&unknown),
        Err(ResumeError::Malformed { .. })
    ));
    let mut value: Value =
        serde_json::from_slice(&serde_json::to_vec(&checkpoint).unwrap()).unwrap();
    value["structure"]["entries"] = json!(["zz"]);
    let broken = serde_json::to_vec(&value).unwrap();
    assert!(matches!(
        Checkpoint::from_json_slice(&broken),
        Err(ResumeError::Malformed { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn given_each_resume_refusal_when_displayed_then_it_has_a_reason() {
    let rig = base();
    let checkpoint = checkpoint_of(&rig).await;
    let errors = [
        refusal(&chain("2", &["a", "b"]), checkpoint.clone()).await,
        refusal(&chain("1", &["a"]), checkpoint.clone()).await,
        refusal(&rig, forged(&checkpoint, |v| v["nodes"] = json!([]))).await,
        refusal(&rig, forged(&checkpoint, |v| v["nodes"] = json!(["zz"]))).await,
        refusal(
            &rig,
            forged(&checkpoint, |v| v["nodes"] = json!(["a", "a"])),
        )
        .await,
        refusal(&rig, forged(&checkpoint, |v| v["state"] = json!("1"))).await,
        Checkpoint::from_json_slice(b"nope").unwrap_err(),
    ];
    for error in errors {
        assert!(!error.to_string().is_empty());
    }
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
