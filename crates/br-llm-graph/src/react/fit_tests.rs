//! The check of a structured value against its shape: every kind, nesting,
//! required and undeclared names, and the reason given for each misfit.
//! Like the check, the tests use br-llm-clients-core and serde_json alone.

use br_llm_clients_core::{Parameter, ParameterKind, Parameters, StructuredShape};
use serde_json::{Value as Json, json};

use super::fit;

fn parameter(name: &str, required: bool, kind: ParameterKind) -> Parameter {
    Parameter::new(name, "", required, kind).unwrap()
}

fn parameters(list: Vec<Parameter>) -> Parameters {
    Parameters::new(list).unwrap()
}

fn shape(list: Vec<Parameter>) -> StructuredShape {
    StructuredShape::new("answer", None, parameters(list)).unwrap()
}

/// A shape holding one required parameter `value` of `kind`.
fn one(kind: ParameterKind) -> StructuredShape {
    shape(vec![parameter("value", true, kind)])
}

/// The reason `value` does not fit `shape`, `None` when it fits.
fn reason(shape: &StructuredShape, value: Json) -> Option<String> {
    fit(shape, &value).err().map(|misfit| misfit.to_string())
}

fn tone() -> ParameterKind {
    ParameterKind::Enum {
        values: vec!["formal".to_owned(), "casual".to_owned()],
    }
}

#[test]
fn given_each_kind_when_its_value_is_given_then_it_fits() {
    let fitting = [
        (ParameterKind::String, json!("tides")),
        (ParameterKind::Number, json!(1.5)),
        (ParameterKind::Number, json!(2)),
        (ParameterKind::Integer, json!(-3)),
        (ParameterKind::Integer, json!(u64::MAX)),
        (ParameterKind::Boolean, json!(false)),
        (tone(), json!("casual")),
        (
            ParameterKind::Array {
                items: Box::new(ParameterKind::String),
            },
            json!(["a", "b"]),
        ),
        (
            ParameterKind::Array {
                items: Box::new(ParameterKind::String),
            },
            json!([]),
        ),
        (
            ParameterKind::Object {
                parameters: parameters(vec![parameter("url", true, ParameterKind::String)]),
            },
            json!({"url": "https://example.org"}),
        ),
    ];
    for (kind, value) in fitting {
        assert_eq!(reason(&one(kind), json!({ "value": value })), None);
    }
}

#[test]
fn given_each_kind_when_another_value_is_given_then_the_reason_names_the_parameter_and_the_kind() {
    let misfits = [
        (ParameterKind::String, json!(3), "`value` is not a string"),
        (ParameterKind::Number, json!("3"), "`value` is not a number"),
        (
            ParameterKind::Boolean,
            json!("true"),
            "`value` is not a boolean",
        ),
        (
            tone(),
            json!("rude"),
            "`value` is not one of `formal`, `casual`",
        ),
        (tone(), json!(1), "`value` is not one of `formal`, `casual`"),
        (
            ParameterKind::Array {
                items: Box::new(ParameterKind::String),
            },
            json!("a"),
            "`value` is not an array",
        ),
        (
            ParameterKind::Object {
                parameters: Parameters::default(),
            },
            json!([]),
            "`value` is not an object",
        ),
    ];
    for (kind, value, expected) in misfits {
        assert_eq!(
            reason(&one(kind), json!({ "value": value })).as_deref(),
            Some(expected)
        );
    }
}

#[test]
fn given_an_integer_parameter_when_the_value_is_a_float_then_it_does_not_fit_even_when_whole() {
    for value in [json!(1.0), json!(2.5), json!("2")] {
        assert_eq!(
            reason(&one(ParameterKind::Integer), json!({ "value": value })).as_deref(),
            Some("`value` is not an integer")
        );
    }
}

#[test]
fn given_a_value_that_is_not_an_object_when_checked_then_the_value_itself_is_named() {
    for value in [json!("answer"), json!([1]), json!(null), json!(4)] {
        assert_eq!(
            reason(&one(ParameterKind::String), value).as_deref(),
            Some("the value is not an object")
        );
    }
}

#[test]
fn given_a_required_parameter_missing_or_null_when_checked_then_it_is_missing_or_null() {
    let shape = one(ParameterKind::String);
    for value in [json!({}), json!({ "value": null })] {
        assert_eq!(
            reason(&shape, value).as_deref(),
            Some("`value` is missing or null")
        );
    }
}

#[test]
fn given_an_optional_parameter_absent_or_null_when_checked_then_it_fits() {
    let shape = shape(vec![
        parameter("value", true, ParameterKind::String),
        parameter("note", false, ParameterKind::String),
    ]);
    assert_eq!(reason(&shape, json!({ "value": "a" })), None);
    assert_eq!(reason(&shape, json!({ "value": "a", "note": null })), None);
    assert_eq!(
        reason(&shape, json!({ "value": "a", "note": 1 })).as_deref(),
        Some("`note` is not a string")
    );
}

#[test]
fn given_a_name_the_shape_does_not_declare_when_checked_then_it_is_not_in_the_shape() {
    let shape = one(ParameterKind::String);
    assert_eq!(
        reason(&shape, json!({ "value": "a", "extra": true })).as_deref(),
        Some("`extra` is not in the shape")
    );
    let empty = StructuredShape::new("empty", None, Parameters::default()).unwrap();
    assert_eq!(reason(&empty, json!({})), None);
    assert_eq!(
        reason(&empty, json!({ "a": 1 })).as_deref(),
        Some("`a` is not in the shape")
    );
}

#[test]
fn given_several_misfits_when_checked_then_the_declared_parameters_come_first_in_their_order() {
    let shape = shape(vec![
        parameter("first", true, ParameterKind::String),
        parameter("second", true, ParameterKind::Boolean),
    ]);
    assert_eq!(
        reason(&shape, json!({ "extra": 1, "second": 2 })).as_deref(),
        Some("`first` is missing or null")
    );
    assert_eq!(
        reason(&shape, json!({ "extra": 1, "first": "a", "second": 2 })).as_deref(),
        Some("`second` is not a boolean")
    );
    assert_eq!(
        reason(&shape, json!({ "extra": 1, "first": "a", "second": true })).as_deref(),
        Some("`extra` is not in the shape")
    );
}

#[test]
fn given_nested_objects_and_arrays_when_an_item_does_not_fit_then_its_path_is_named() {
    let source = ParameterKind::Object {
        parameters: parameters(vec![
            parameter("url", true, ParameterKind::String),
            parameter(
                "pages",
                false,
                ParameterKind::Array {
                    items: Box::new(ParameterKind::Integer),
                },
            ),
        ]),
    };
    let shape = shape(vec![parameter(
        "sources",
        true,
        ParameterKind::Array {
            items: Box::new(source),
        },
    )]);
    let cases = [
        (
            json!({ "sources": [{ "url": "a" }, { "url": "b", "pages": [1, 2] }] }),
            None,
        ),
        (
            json!({ "sources": [{ "url": "a" }, { "pages": [] }] }),
            Some("`sources[1].url` is missing or null"),
        ),
        (
            json!({ "sources": [{ "url": "a", "pages": [1, "2"] }] }),
            Some("`sources[0].pages[1]` is not an integer"),
        ),
        (
            json!({ "sources": [{ "url": "a", "title": "t" }] }),
            Some("`sources[0].title` is not in the shape"),
        ),
        (
            json!({ "sources": ["a"] }),
            Some("`sources[0]` is not an object"),
        ),
    ];
    for (value, expected) in cases {
        assert_eq!(reason(&shape, value).as_deref(), expected);
    }
}

#[test]
fn given_arrays_of_arrays_when_an_inner_item_does_not_fit_then_each_index_is_named() {
    let shape = one(ParameterKind::Array {
        items: Box::new(ParameterKind::Array {
            items: Box::new(ParameterKind::Boolean),
        }),
    });
    assert_eq!(reason(&shape, json!({ "value": [[true], []] })), None);
    assert_eq!(
        reason(&shape, json!({ "value": [[true], [false, null]] })).as_deref(),
        Some("`value[1][1]` is not a boolean")
    );
}

#[test]
fn given_a_misfit_when_its_reason_is_read_then_no_value_found_is_quoted() {
    let shape = one(ParameterKind::Integer);
    let reason = reason(&shape, json!({ "value": "a secret the model wrote" })).unwrap();
    assert!(!reason.contains("secret"), "{reason}");
}
