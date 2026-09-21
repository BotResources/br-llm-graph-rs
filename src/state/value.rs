use br_llm_messages::Conversation;

use crate::error::GraphError;
use crate::state::kind::Kind;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(into = "f64")]
pub struct Finite(f64);

impl Finite {
    pub fn new(value: f64) -> Result<Self, GraphError> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err(GraphError::FloatNotFinite)
        }
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

impl From<Finite> for f64 {
    fn from(value: Finite) -> Self {
        value.0
    }
}

impl<'de> serde::Deserialize<'de> for Finite {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = f64::deserialize(deserializer)?;
        Finite::new(raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Value {
    Int(i64),
    Float(Finite),
    Str(String),
    Bool(bool),
    List(Vec<Value>),
    Conversation(Conversation),
}

#[derive(serde::Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
enum RawValue {
    Int(i64),
    Float(Finite),
    Str(String),
    Bool(bool),
    List(Vec<Value>),
    Conversation(Conversation),
}

impl From<RawValue> for Value {
    fn from(raw: RawValue) -> Self {
        match raw {
            RawValue::Int(v) => Value::Int(v),
            RawValue::Float(v) => Value::Float(v),
            RawValue::Str(v) => Value::Str(v),
            RawValue::Bool(v) => Value::Bool(v),
            RawValue::List(v) => Value::List(v),
            RawValue::Conversation(v) => Value::Conversation(v),
        }
    }
}

impl<'de> serde::Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Value::from(RawValue::deserialize(deserializer)?))
    }
}

impl Value {
    pub fn int(value: i64) -> Self {
        Value::Int(value)
    }

    pub fn float(value: f64) -> Result<Self, GraphError> {
        Ok(Value::Float(Finite::new(value)?))
    }

    pub fn str(value: impl Into<String>) -> Self {
        Value::Str(value.into())
    }

    pub fn bool(value: bool) -> Self {
        Value::Bool(value)
    }

    pub fn list(values: Vec<Value>) -> Self {
        Value::List(values)
    }

    pub fn conversation(conversation: Conversation) -> Self {
        Value::Conversation(conversation)
    }

    pub fn tag(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "str",
            Value::Bool(_) => "bool",
            Value::List(_) => "list",
            Value::Conversation(_) => "conversation",
        }
    }

    pub fn matches(&self, kind: &Kind) -> bool {
        match (self, kind) {
            (Value::Int(_), Kind::Int) => true,
            (Value::Float(_), Kind::Float) => true,
            (Value::Str(_), Kind::Str) => true,
            (Value::Bool(_), Kind::Bool) => true,
            (Value::Conversation(_), Kind::Conversation) => true,
            (Value::List(items), Kind::List { element }) => {
                items.iter().all(|item| item.matches(element))
            }
            (
                Value::Int(_)
                | Value::Float(_)
                | Value::Str(_)
                | Value::Bool(_)
                | Value::List(_)
                | Value::Conversation(_),
                _,
            ) => false,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            Value::Float(v) => write!(f, "{}", v.get()),
            Value::Str(v) => write!(f, "{v:?}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::List(items) => {
                f.write_str("[")?;
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            Value::Conversation(conversation) => {
                write!(f, "conversation({} entries)", conversation.entries().len())
            }
        }
    }
}

#[cfg(test)]
#[path = "value_tests.rs"]
mod tests;
