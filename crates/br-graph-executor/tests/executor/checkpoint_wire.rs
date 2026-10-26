use br_graph_executor::{Checkpoint, ResumeError};
use serde::{Deserialize, Serialize};

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;

async fn checkpoint_of_a_failed_a(rig: &Rig, initial: br_graph_contract::State<Kv>) -> Checkpoint {
    let host = begin(rig, initial, &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.failed("boom");
    failed(host.outcome().await).1
}

fn a_then_b() -> Rig {
    chain("1", &["a", "b"])
}

#[derive(Serialize, Deserialize)]
struct Stored {
    run: String,
    #[serde(flatten)]
    checkpoint: Checkpoint,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind")]
enum Record {
    Paused { checkpoint: Checkpoint },
    Resumable(Checkpoint),
}

#[tokio::test(start_paused = true)]
async fn given_a_checkpoint_when_written_and_read_as_json_then_it_equals_the_original() {
    let rig = a_then_b();
    let checkpoint = checkpoint_of_a_failed_a(&rig, state(&[("k", 1)])).await;
    let text = serde_json::to_string(&checkpoint).unwrap();
    let from_text: Checkpoint = serde_json::from_str(&text).unwrap();
    let from_slice = Checkpoint::from_json_slice(text.as_bytes()).unwrap();
    assert_eq!(from_text, checkpoint);
    assert_eq!(from_slice, checkpoint);
    assert_eq!(from_slice.state_json(), checkpoint.state_json());
    assert_eq!(state_of(&from_text), state_of(&checkpoint));
}

#[tokio::test(start_paused = true)]
async fn given_a_checkpoint_when_converted_to_a_json_value_and_back_then_it_equals_the_original() {
    let rig = a_then_b();
    let checkpoint = checkpoint_of_a_failed_a(&rig, state(&[("k", 1)])).await;
    let value = serde_json::to_value(&checkpoint).unwrap();
    let back: Checkpoint = serde_json::from_value(value).unwrap();
    assert_eq!(back, checkpoint);
}

#[tokio::test(start_paused = true)]
async fn given_a_checkpoint_flattened_into_a_host_record_when_round_tripped_then_it_equals_the_original()
 {
    let rig = a_then_b();
    let checkpoint = checkpoint_of_a_failed_a(&rig, state(&[("k", 1)])).await;
    let stored = Stored {
        run: "run-1".to_owned(),
        checkpoint: checkpoint.clone(),
    };
    let text = serde_json::to_string(&stored).unwrap();
    let back: Stored = serde_json::from_str(&text).unwrap();
    assert_eq!(back.run, "run-1");
    assert_eq!(back.checkpoint, checkpoint);
}

#[tokio::test(start_paused = true)]
async fn given_a_checkpoint_inside_an_internally_tagged_enum_when_round_tripped_then_it_equals_the_original()
 {
    let rig = a_then_b();
    let checkpoint = checkpoint_of_a_failed_a(&rig, state(&[("k", 1)])).await;
    let paused = serde_json::to_string(&Record::Paused {
        checkpoint: checkpoint.clone(),
    })
    .unwrap();
    let resumable = serde_json::to_string(&Record::Resumable(checkpoint.clone())).unwrap();
    match serde_json::from_str::<Record>(&paused).unwrap() {
        Record::Paused { checkpoint: back } => assert_eq!(back, checkpoint),
        Record::Resumable(_) => panic!("wrong variant"),
    }
    match serde_json::from_str::<Record>(&resumable).unwrap() {
        Record::Resumable(back) => assert_eq!(back, checkpoint),
        Record::Paused { .. } => panic!("wrong variant"),
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_checkpoint_document_with_an_unknown_field_when_read_then_it_is_refused() {
    let rig = a_then_b();
    let checkpoint = checkpoint_of_a_failed_a(&rig, state(&[])).await;
    let mut value = serde_json::to_value(&checkpoint).unwrap();
    value["surprise"] = serde_json::json!(true);
    let text = value.to_string();
    assert!(matches!(
        Checkpoint::from_json_slice(text.as_bytes()),
        Err(ResumeError::Malformed { .. })
    ));
    assert!(serde_json::from_str::<Checkpoint>(&text).is_err());
    assert!(serde_json::from_value::<Checkpoint>(value).is_err());
}

#[tokio::test(start_paused = true)]
async fn given_a_deserialized_checkpoint_when_resumed_then_the_run_reaches_the_same_final_state() {
    let rig = a_then_b();
    let checkpoint = checkpoint_of_a_failed_a(&rig, state(&[("k", 1)])).await;
    let text = serde_json::to_string(&checkpoint).unwrap();
    let revived: Checkpoint = serde_json::from_str(&text).unwrap();
    assert_eq!(state_of(&revived), state_of(&checkpoint));
    let resumed = begin_resume(&rig, revived, BIG, Setup::new());
    let mut a = rig.stage.instance("a").await;
    assert_eq!(a.accepted_inputs(), vec![kv(&[("k", 1)])]);
    a.result(set("j", 2));
    let mut b = rig.stage.instance("b").await;
    b.result(Change::Empty);
    assert_eq!(ended(resumed.outcome().await), kv(&[("j", 2), ("k", 1)]));
}

#[tokio::test(start_paused = true)]
async fn given_a_state_with_a_number_beyond_64_bits_when_written_and_read_by_the_checked_door_then_the_resumed_state_is_equal()
 {
    let rig = a_then_b();
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.result(Change::Wide(u128::MAX));
    let mut b = rig.stage.instance("b").await;
    b.failed("boom");
    let (_, checkpoint) = failed(host.outcome().await);
    let text = serde_json::to_string(&checkpoint).unwrap();
    let revived = Checkpoint::from_json_slice(text.as_bytes()).unwrap();
    assert_eq!(state_of(&revived).wide, u128::MAX);
    let resumed = begin_resume(&rig, revived, BIG, Setup::new());
    let mut b_again = rig.stage.instance("b").await;
    let given = b_again.accepted_inputs();
    assert_eq!(given.len(), 1);
    assert_eq!(given[0].wide, u128::MAX);
    b_again.result(Change::Empty);
    assert_eq!(ended(resumed.outcome().await).wide, u128::MAX);
}
