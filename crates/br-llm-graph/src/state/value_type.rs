use br_llm_messages::Conversation;

use crate::error::GraphError;
use crate::state::kind::Kind;
use crate::state::value::{Finite, Value};
use crate::value::Key;

mod sealed {
    pub trait Sealed {}

    impl Sealed for i64 {}
    impl Sealed for f64 {}
    impl Sealed for String {}
    impl Sealed for bool {}
    impl<T: Sealed> Sealed for Vec<T> {}
    impl Sealed for br_llm_messages::Conversation {}
}

/// The Rust type of a kind: what a typed key carries.
///
/// | Kind | Rust type |
/// |---|---|
/// | `int` | `i64` |
/// | `float` | `f64` |
/// | `str` | `String` |
/// | `bool` | `bool` |
/// | `list<element>` | `Vec<T>`, `T` the element's Rust type |
/// | `conversation` | `Conversation` (br-llm-messages) |
///
/// Only these types implement it (the trait is sealed), so a key of any other
/// type cannot be declared. What the type cannot say is checked when a value
/// is built from it: a float must be finite (`FloatNotFinite`).
pub trait ValueType: sealed::Sealed + Sized + Send + Sync + 'static {
    /// What a read through a key of this type gives: the value itself for an
    /// int, a float and a bool, a borrow for a string (`&str`) and a
    /// conversation, a vector of the elements' reads for a list.
    type Read<'a>;

    /// The kind of the values of this type.
    fn kind() -> Kind;

    /// `value` read as this type: `None` when it holds another kind.
    fn read(value: &Value) -> Option<Self::Read<'_>>;

    /// The value holding `self`. A float that is not finite is refused
    /// (`FloatNotFinite`), in a list too.
    fn into_value(self) -> Result<Value, GraphError>;
}

impl ValueType for i64 {
    type Read<'a> = i64;

    fn kind() -> Kind {
        Kind::Int
    }

    fn read(value: &Value) -> Option<i64> {
        match value {
            Value::Int(v) => Some(*v),
            _ => None,
        }
    }

    fn into_value(self) -> Result<Value, GraphError> {
        Ok(Value::Int(self))
    }
}

impl ValueType for f64 {
    type Read<'a> = f64;

    fn kind() -> Kind {
        Kind::Float
    }

    fn read(value: &Value) -> Option<f64> {
        match value {
            Value::Float(v) => Some(v.get()),
            _ => None,
        }
    }

    fn into_value(self) -> Result<Value, GraphError> {
        Ok(Value::Float(Finite::new(self)?))
    }
}

impl ValueType for String {
    type Read<'a> = &'a str;

    fn kind() -> Kind {
        Kind::Str
    }

    fn read(value: &Value) -> Option<&str> {
        match value {
            Value::Str(v) => Some(v),
            _ => None,
        }
    }

    fn into_value(self) -> Result<Value, GraphError> {
        Ok(Value::Str(self))
    }
}

impl ValueType for bool {
    type Read<'a> = bool;

    fn kind() -> Kind {
        Kind::Bool
    }

    fn read(value: &Value) -> Option<bool> {
        match value {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }

    fn into_value(self) -> Result<Value, GraphError> {
        Ok(Value::Bool(self))
    }
}

impl<T: ValueType> ValueType for Vec<T> {
    type Read<'a> = Vec<T::Read<'a>>;

    fn kind() -> Kind {
        Kind::list(T::kind())
    }

    fn read(value: &Value) -> Option<Vec<T::Read<'_>>> {
        match value {
            Value::List(items) => items.iter().map(T::read).collect(),
            _ => None,
        }
    }

    fn into_value(self) -> Result<Value, GraphError> {
        let items = self
            .into_iter()
            .map(T::into_value)
            .collect::<Result<Vec<Value>, GraphError>>()?;
        Ok(Value::List(items))
    }
}

impl ValueType for Conversation {
    type Read<'a> = &'a Conversation;

    fn kind() -> Kind {
        Kind::Conversation
    }

    fn read(value: &Value) -> Option<&Conversation> {
        match value {
            Value::Conversation(conversation) => Some(conversation),
            _ => None,
        }
    }

    fn into_value(self) -> Result<Value, GraphError> {
        Ok(Value::Conversation(self))
    }
}

/// `value`, the value of `key` declared with the kind `declared`, read as
/// `T`. A key declared with another kind than `T`'s is refused
/// (`KindMismatch`, naming `T`'s kind and the type of the value found), even
/// when its value would read as `T` (an empty list of another element kind).
pub(crate) fn read_as<'a, T: ValueType>(
    key: &Key,
    declared: &Kind,
    value: &'a Value,
) -> Result<T::Read<'a>, GraphError> {
    let mismatch = || GraphError::KindMismatch {
        key: key.clone(),
        expected: T::kind(),
        found: value.tag(),
    };
    if *declared != T::kind() {
        return Err(mismatch());
    }
    T::read(value).ok_or_else(mismatch)
}

#[cfg(test)]
#[path = "value_type_tests.rs"]
mod tests;
