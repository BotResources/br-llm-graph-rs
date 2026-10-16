use br_llm_messages::Conversation;

use super::*;

#[test]
fn given_each_rust_type_when_asked_its_kind_then_the_kind_of_the_table() {
    assert_eq!(i64::kind(), Kind::Int);
    assert_eq!(f64::kind(), Kind::Float);
    assert_eq!(String::kind(), Kind::Str);
    assert_eq!(bool::kind(), Kind::Bool);
    assert_eq!(Vec::<String>::kind(), Kind::list(Kind::Str));
    assert_eq!(
        Vec::<Vec<Conversation>>::kind(),
        Kind::list(Kind::list(Kind::Conversation))
    );
    assert_eq!(Conversation::kind(), Kind::Conversation);
}

#[test]
fn given_a_value_built_from_a_rust_value_when_read_back_then_the_same_and_of_its_kind() {
    let value = vec![vec![1_i64, 2], vec![]].into_value().unwrap();
    assert!(value.matches(&Vec::<Vec<i64>>::kind()));
    assert_eq!(
        Vec::<Vec<i64>>::read(&value),
        Some(vec![vec![1, 2], vec![]])
    );
    let text = "hi".to_owned().into_value().unwrap();
    assert_eq!(String::read(&text), Some("hi"));
    assert_eq!(f64::read(&0.5.into_value().unwrap()), Some(0.5));
    assert_eq!(bool::read(&true.into_value().unwrap()), Some(true));
}

#[test]
fn given_a_value_of_another_kind_when_read_then_none() {
    assert_eq!(i64::read(&Value::str("1")), None);
    assert_eq!(String::read(&Value::int(1)), None);
    assert!(Conversation::read(&Value::bool(true)).is_none());
    assert_eq!(
        Vec::<i64>::read(&Value::list(vec![Value::int(1), Value::str("2")])),
        None
    );
}

#[test]
fn given_a_float_that_is_not_finite_when_its_value_is_built_then_float_not_finite() {
    assert!(matches!(
        f64::NAN.into_value(),
        Err(GraphError::FloatNotFinite)
    ));
    assert!(matches!(
        vec![vec![1.0], vec![f64::INFINITY]].into_value(),
        Err(GraphError::FloatNotFinite)
    ));
}
