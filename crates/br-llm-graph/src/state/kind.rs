use br_llm_messages::Conversation;

use crate::state::value::{Finite, Value};

/// The kind of a key: what values it holds. Its JSON is `{"type"}`, with
/// `element` for a list.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    /// 64-bit signed integers; neutral value 0.
    Int,
    /// Finite floats; neutral value 0.0.
    Float,
    /// Strings; neutral value the empty string.
    Str,
    /// Booleans; neutral value false.
    Bool,
    /// Lists whose every element has the element kind; neutral value the empty
    /// list.
    List {
        /// The kind of the elements.
        element: Box<Kind>,
    },
    /// br-llm-messages conversations; neutral value the empty conversation.
    Conversation,
}

impl Kind {
    /// The kind of lists of `element`.
    pub fn list(element: Kind) -> Self {
        Kind::List {
            element: Box::new(element),
        }
    }

    /// The type name a value of this kind carries (`Value::tag`): what a
    /// check made at build names as found, before any value exists.
    pub(crate) fn tag(&self) -> &'static str {
        match self {
            Kind::Int => "int",
            Kind::Float => "float",
            Kind::Str => "str",
            Kind::Bool => "bool",
            Kind::List { .. } => "list",
            Kind::Conversation => "conversation",
        }
    }

    /// The value a key of this kind holds when nothing else is given: empty
    /// string, 0, 0.0, false, empty list, empty conversation.
    pub fn neutral(&self) -> Value {
        match self {
            Kind::Int => Value::Int(0),
            Kind::Float => Value::Float(Finite::ZERO),
            Kind::Str => Value::Str(String::new()),
            Kind::Bool => Value::Bool(false),
            Kind::List { .. } => Value::List(Vec::new()),
            Kind::Conversation => Value::Conversation(Conversation::new()),
        }
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Kind::Int => f.write_str("int"),
            Kind::Float => f.write_str("float"),
            Kind::Str => f.write_str("str"),
            Kind::Bool => f.write_str("bool"),
            Kind::List { element } => write!(f, "list<{element}>"),
            Kind::Conversation => f.write_str("conversation"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_nested_list_kind_when_round_tripped_then_identical() {
        let kind = Kind::list(Kind::list(Kind::Str));
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(serde_json::from_str::<Kind>(&json).unwrap(), kind);
    }

    #[test]
    fn given_each_kind_when_neutral_then_the_empty_value_of_that_kind() {
        let cases = [
            (Kind::Int, Value::int(0)),
            (Kind::Float, Value::float(0.0).unwrap()),
            (Kind::Str, Value::str("")),
            (Kind::Bool, Value::bool(false)),
            (Kind::list(Kind::Int), Value::list(Vec::new())),
            (Kind::Conversation, Value::conversation(Conversation::new())),
        ];
        for (kind, expected) in cases {
            let neutral = kind.neutral();
            assert!(neutral.matches(&kind));
            assert_eq!(neutral, expected);
        }
    }

    #[test]
    fn given_each_kind_when_tagged_then_the_tag_of_its_values() {
        for kind in [
            Kind::Int,
            Kind::Float,
            Kind::Str,
            Kind::Bool,
            Kind::list(Kind::Int),
            Kind::Conversation,
        ] {
            assert_eq!(kind.tag(), kind.neutral().tag());
        }
    }

    #[test]
    fn given_scalar_kind_when_serialized_then_type_tagged() {
        assert_eq!(
            serde_json::to_value(&Kind::Int).unwrap(),
            serde_json::json!({ "type": "int" })
        );
    }
}
