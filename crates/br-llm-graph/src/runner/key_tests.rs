//! The call key's canonical arguments: one text for equal arguments,
//! whatever the order they were built in, another for other arguments.

use serde_json::{Map, json};

use crate::runner::test_support::{call_key, object};

#[test]
fn given_equal_arguments_built_in_another_order_when_keyed_then_one_key() {
    let mut first = Map::new();
    first.insert("question".to_owned(), json!("tides?"));
    first.insert("filters".to_owned(), json!({"lang": "fr", "year": 2026}));
    let mut second = Map::new();
    second.insert("filters".to_owned(), json!({"year": 2026, "lang": "fr"}));
    second.insert("question".to_owned(), json!("tides?"));
    let (first, second) = (
        call_key("r", "ask", None, &first),
        call_key("r", "ask", None, &second),
    );

    assert_eq!(first, second);
    assert_eq!(
        first.arguments,
        r#"{"filters":{"lang":"fr","year":2026},"question":"tides?"}"#
    );
}

#[test]
fn given_other_arguments_when_keyed_then_another_key() {
    let asked = |question: &str| object(json!({"question": question, "ids": ["d1", "d\"2"]}));
    let key = call_key("r", "ask", None, &asked("tides?"));

    assert_ne!(key, call_key("r", "ask", None, &asked("moons?")));
    assert_eq!(
        key.arguments,
        r#"{"ids":["d1","d\"2"],"question":"tides?"}"#
    );
    assert_eq!(call_key("r", "ask", None, &Map::new()).arguments, "{}");
}
