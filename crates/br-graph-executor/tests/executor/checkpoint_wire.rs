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

const EXACT_STATE: &str =
    r#"{"wide":340282366920938463463374607431768211455,"values":{"b":2,"a":1},"pad":""}"#;

fn exact_checkpoint(rig: &Rig) -> Checkpoint {
    handwritten_checkpoint(rig, EXACT_STATE, &["b"])
}

async fn final_state_of_resumed(checkpoint: Checkpoint) -> Kv {
    let rig = a_then_b();
    let resumed = begin_resume(&rig, checkpoint, BIG, Setup::new());
    let mut b = rig.stage.instance("b").await;
    assert_eq!(b.accepted_inputs().len(), 1);
    b.result(Change::Empty);
    ended(resumed.outcome().await)
}

async fn assert_resumed_exactly(door: &str, checkpoint: Checkpoint) {
    assert_eq!(checkpoint.state_json(), EXACT_STATE, "{door}");
    let end = final_state_of_resumed(checkpoint).await;
    assert_eq!(end.wide, u128::MAX, "{door}");
    assert_eq!(end.values, kv(&[("a", 1), ("b", 2)]).values, "{door}");
}

#[tokio::test(start_paused = true)]
async fn given_the_wire_form_of_a_checkpoint_when_read_then_the_state_is_a_json_string_with_the_exact_text()
 {
    let rig = a_then_b();
    let checkpoint = exact_checkpoint(&rig);
    let value = serde_json::to_value(&checkpoint).unwrap();
    assert_eq!(value["state"], serde_json::json!(EXACT_STATE));
    let text = serde_json::to_string(&checkpoint).unwrap();
    let document: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(document["state"].as_str(), Some(EXACT_STATE));
}

#[tokio::test(start_paused = true)]
async fn given_a_state_that_is_not_a_json_string_when_read_then_the_checkpoint_is_malformed() {
    let rig = a_then_b();
    let mut value = serde_json::to_value(exact_checkpoint(&rig)).unwrap();
    for bad in [
        serde_json::json!({"values": {}, "pad": ""}),
        serde_json::json!(12),
        serde_json::json!(null),
    ] {
        value["state"] = bad;
        let text = value.to_string();
        assert!(matches!(
            Checkpoint::from_json_slice(text.as_bytes()),
            Err(ResumeError::Malformed { .. })
        ));
        assert!(serde_json::from_str::<Checkpoint>(&text).is_err());
        assert!(serde_json::from_value::<Checkpoint>(value.clone()).is_err());
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_number_beyond_64_bits_and_unsorted_keys_when_read_by_the_checked_door_then_the_resumed_state_is_exact()
 {
    let rig = a_then_b();
    let text = serde_json::to_string(&exact_checkpoint(&rig)).unwrap();
    let revived = Checkpoint::from_json_slice(text.as_bytes()).unwrap();
    assert_resumed_exactly("from_json_slice", revived).await;
}

#[tokio::test(start_paused = true)]
async fn given_a_number_beyond_64_bits_and_unsorted_keys_when_read_by_from_str_or_from_value_then_the_resumed_state_is_exact()
 {
    let rig = a_then_b();
    let checkpoint = exact_checkpoint(&rig);
    let text = serde_json::to_string(&checkpoint).unwrap();
    let from_str: Checkpoint = serde_json::from_str(&text).unwrap();
    assert_resumed_exactly("from_str", from_str).await;
    let value = serde_json::to_value(&checkpoint).unwrap();
    let from_value: Checkpoint = serde_json::from_value(value).unwrap();
    assert_resumed_exactly("from_value", from_value).await;
}

#[tokio::test(start_paused = true)]
async fn given_a_number_beyond_64_bits_and_unsorted_keys_when_read_through_flatten_or_a_tagged_enum_then_the_resumed_state_is_exact()
 {
    let rig = a_then_b();
    let checkpoint = exact_checkpoint(&rig);
    let stored = serde_json::to_string(&Stored {
        run: "run-1".to_owned(),
        checkpoint: checkpoint.clone(),
    })
    .unwrap();
    let flattened: Stored = serde_json::from_str(&stored).unwrap();
    assert_resumed_exactly("flatten", flattened.checkpoint).await;
    let record = serde_json::to_string(&Record::Resumable(checkpoint.clone())).unwrap();
    match serde_json::from_str::<Record>(&record).unwrap() {
        Record::Resumable(back) => assert_resumed_exactly("tagged enum", back).await,
        Record::Paused { .. } => panic!("wrong variant"),
    }
    let paused = serde_json::to_string(&Record::Paused { checkpoint }).unwrap();
    match serde_json::from_str::<Record>(&paused).unwrap() {
        Record::Paused { checkpoint: back } => assert_resumed_exactly("tagged field", back).await,
        Record::Resumable(_) => panic!("wrong variant"),
    }
}

#[tokio::test(start_paused = true)]
async fn given_two_checkpoints_whose_state_texts_differ_only_by_key_order_when_compared_then_they_are_different()
 {
    let rig = a_then_b();
    let sorted = handwritten_checkpoint(&rig, r#"{"values":{"a":1,"b":2},"pad":""}"#, &["b"]);
    let unsorted = handwritten_checkpoint(&rig, r#"{"values":{"b":2,"a":1},"pad":""}"#, &["b"]);
    assert_ne!(sorted, unsorted);
    assert_eq!(sorted, sorted.clone());
}

#[tokio::test(start_paused = true)]
async fn given_a_run_that_holds_u128_max_when_its_checkpoint_is_read_by_any_door_then_the_state_is_exact()
 {
    let rig = a_then_b();
    let host = begin(&rig, state(&[("z", 1), ("a", 2)]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.result(Change::Wide(u128::MAX));
    let mut b = rig.stage.instance("b").await;
    b.failed("boom");
    let (_, checkpoint) = failed(host.outcome().await);
    let original = checkpoint.state_json().to_owned();
    let text = serde_json::to_string(&checkpoint).unwrap();
    let value = serde_json::to_value(&checkpoint).unwrap();
    let revived = [
        Checkpoint::from_json_slice(text.as_bytes()).unwrap(),
        serde_json::from_str(&text).unwrap(),
        serde_json::from_value(value).unwrap(),
    ];
    for door in revived {
        assert_eq!(door.state_json(), original);
        assert_eq!(state_of(&door).wide, u128::MAX);
        assert_eq!(door, checkpoint);
    }
}
