use br_graph_contract::State;
use br_graph_executor::{Checkpoint, Outcome, RunFailure};

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;

const MARKER: &str = "MARKER-7f3a9c";

fn checkpoint_holding_the_marker() -> Checkpoint {
    let rig = chain("1", &["a", "b"]);
    let document = format!(r#"{{"values":{{}},"pad":"{MARKER}"}}"#);
    handwritten_checkpoint(&rig, &document, &["a"])
}

#[test]
fn given_a_checkpoint_whose_state_holds_a_marker_when_debug_printed_then_the_marker_is_absent_and_the_summary_is_shown()
 {
    let checkpoint = checkpoint_holding_the_marker();

    let text = format!("{checkpoint:?}");

    assert!(!text.contains(MARKER));
    assert!(text.contains(&format!("state_size: {}", checkpoint.state_json().len())));
    assert!(text.contains("nodes: [\"a\"]"));
}

#[test]
fn given_a_failed_outcome_when_debug_printed_then_the_failures_are_shown_and_the_marker_is_absent()
{
    let outcome: Outcome<Kv> = Outcome::RunFailed {
        failures: vec![RunFailure::NodeRefusedInput {
            node: "a".to_owned(),
            reason: "closed".to_owned(),
        }],
        checkpoint: checkpoint_holding_the_marker(),
    };

    let text = format!("{outcome:?}");

    assert!(text.starts_with("RunFailed"));
    assert!(text.contains("closed"));
    assert!(!text.contains(MARKER));
}

#[test]
fn given_a_cancelled_outcome_when_debug_printed_then_the_failures_are_shown_and_the_marker_is_absent()
 {
    let outcome: Outcome<Kv> = Outcome::RunCancelled {
        failures: vec![RunFailure::CheckpointNotBuilt {
            reason: "unreadable".to_owned(),
        }],
        checkpoint: checkpoint_holding_the_marker(),
    };

    let text = format!("{outcome:?}");

    assert!(text.starts_with("RunCancelled"));
    assert!(text.contains("unreadable"));
    assert!(!text.contains(MARKER));
}

#[test]
fn given_an_ended_outcome_whose_state_holds_a_marker_when_debug_printed_then_the_marker_is_absent()
{
    let outcome = Outcome::RunEnded {
        state: State::create(Kv {
            pad: MARKER.to_owned(),
            ..kv(&[("k", 1)])
        })
        .unwrap(),
    };

    let text = format!("{outcome:?}");

    assert!(text.starts_with("RunEnded"));
    assert!(!text.contains(MARKER));
}

#[test]
fn given_a_sink_refusal_failure_when_debug_printed_then_the_checkpoint_state_is_absent() {
    let failure = RunFailure::SinkRefused {
        reason: "full".to_owned(),
        checkpoint: checkpoint_holding_the_marker(),
    };

    let text = format!("{failure:?}");

    assert!(text.contains("full"));
    assert!(!text.contains(MARKER));
}
