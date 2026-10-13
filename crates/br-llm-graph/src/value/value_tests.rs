use super::*;
use crate::error::GraphError;

#[test]
fn given_lowercase_snake_when_new_then_accepted() {
    assert_eq!(Key::new("chat_history").unwrap().as_str(), "chat_history");
    assert_eq!(NodeId::new("llm").unwrap().as_str(), "llm");
    assert_eq!(EndLabel::new("done_2").unwrap().as_str(), "done_2");
}

#[test]
fn given_empty_when_new_then_refused() {
    assert!(matches!(
        Key::new(""),
        Err(GraphError::Identifier { field: "key", .. })
    ));
}

#[test]
fn given_leading_digit_when_new_then_refused() {
    assert!(matches!(
        NodeId::new("1node"),
        Err(GraphError::Identifier { .. })
    ));
}

#[test]
fn given_uppercase_when_new_then_refused() {
    assert!(matches!(
        Key::new("Chat"),
        Err(GraphError::Identifier { .. })
    ));
}

#[test]
fn given_dash_when_new_then_refused() {
    assert!(matches!(
        EndLabel::new("gave-up"),
        Err(GraphError::Identifier { .. })
    ));
}

#[test]
fn given_valid_json_string_when_deserialized_then_round_trips() {
    let key = Key::new("num_iter").unwrap();
    let json = serde_json::to_string(&key).unwrap();
    assert_eq!(json, "\"num_iter\"");
    assert_eq!(serde_json::from_str::<Key>(&json).unwrap(), key);
}

#[test]
fn given_illegal_json_string_when_deserialized_then_refused() {
    assert!(serde_json::from_str::<Key>("\"Bad Key\"").is_err());
    assert!(serde_json::from_str::<NodeId>("\"\"").is_err());
}
