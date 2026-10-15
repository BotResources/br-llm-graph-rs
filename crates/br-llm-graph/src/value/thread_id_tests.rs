use super::*;
use crate::error::GraphError;

const V7: &str = "01a1402a-1fc1-7cff-9991-d1720933c79a";

fn refused(text: &str) -> bool {
    matches!(ThreadId::new(text), Err(GraphError::ThreadId { ref value }) if value == text)
}

#[test]
fn given_the_text_of_a_uuidv7_when_new_then_accepted_as_written() {
    let id = ThreadId::new(V7).unwrap();
    assert_eq!(id.as_str(), V7);
    assert_eq!(id.to_string(), V7);
}

#[test]
fn given_each_variant_digit_of_rfc_9562_when_new_then_accepted() {
    for variant in ['8', '9', 'a', 'b'] {
        let text = format!("01a1402a-1fc1-7cff-{variant}991-d1720933c79a");
        assert!(ThreadId::new(text).is_ok(), "variant {variant}");
    }
}

#[test]
fn given_uppercase_hex_digits_when_new_then_accepted_and_kept_as_written() {
    let upper = V7.to_uppercase();
    assert_eq!(ThreadId::new(upper.clone()).unwrap().as_str(), upper);
}

#[test]
fn given_another_uuid_version_when_new_then_refused() {
    assert!(refused("01a1402a-1fc1-4cff-9991-d1720933c79a"));
    assert!(refused("00000000-0000-0000-0000-000000000000"));
    assert!(refused("ffffffff-ffff-ffff-ffff-ffffffffffff"));
}

#[test]
fn given_a_variant_other_than_rfc_9562_when_new_then_refused() {
    assert!(refused("01a1402a-1fc1-7cff-7991-d1720933c79a"));
    assert!(refused("01a1402a-1fc1-7cff-c991-d1720933c79a"));
}

#[test]
fn given_text_that_is_not_the_hyphenated_form_when_new_then_refused() {
    for text in [
        "",
        " ",
        "01a1402a1fc17cff9991d1720933c79a",
        "{01a1402a-1fc1-7cff-9991-d1720933c79a}",
        "urn:uuid:01a1402a-1fc1-7cff-9991-d1720933c79a",
        " 01a1402a-1fc1-7cff-9991-d1720933c79a",
        "01a1402a-1fc1-7cff-9991-d1720933c79a ",
        "01a1402a-1fc1-7cff-9991-d1720933c79",
        "01a1402a-1fc1-7cff-9991-d1720933c79a0",
        "01a1402a-1fc17-cff-9991-d1720933c79a",
        "01a1402g-1fc1-7cff-9991-d1720933c79a",
        "01a1402a_1fc1_7cff_9991_d1720933c79a",
        "01a1402a-1fc1-7cff-9991-d1720933c7\u{e9}",
    ] {
        assert!(refused(text), "{text:?}");
    }
}

#[test]
fn given_a_refused_text_when_displayed_then_the_error_names_it() {
    let error = ThreadId::new("thread-1").unwrap_err();
    assert_eq!(
        error.to_string(),
        "\"thread-1\" is not a thread id: a UUIDv7 written as its text"
    );
}

#[test]
fn given_a_thread_id_when_round_tripped_through_json_then_its_text() {
    let id = ThreadId::new(V7).unwrap();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(json, format!("\"{V7}\""));
    assert_eq!(serde_json::from_str::<ThreadId>(&json).unwrap(), id);
}

#[test]
fn given_json_that_is_not_a_uuidv7_when_deserialized_then_refused_with_the_refusal_message() {
    let error = serde_json::from_str::<ThreadId>("\"thread-1\"").unwrap_err();
    assert!(error.to_string().contains("is not a thread id"), "{error}");
}
