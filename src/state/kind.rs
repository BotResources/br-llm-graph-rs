#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    Int,
    Float,
    Str,
    Bool,
    List { element: Box<Kind> },
    Conversation,
}

impl Kind {
    pub fn list(element: Kind) -> Self {
        Kind::List {
            element: Box::new(element),
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
    fn given_scalar_kind_when_serialized_then_type_tagged() {
        assert_eq!(
            serde_json::to_value(&Kind::Int).unwrap(),
            serde_json::json!({ "type": "int" })
        );
    }
}
