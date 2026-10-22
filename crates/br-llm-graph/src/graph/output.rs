//! Where a node writes a value it gives in the state of the graph it runs in:
//! by name (`Output`), or, for a text, through a typed key (`TextOutput`).

use crate::error::GraphError;
use crate::graph::node::CheckSite;
use crate::state::{Kind, Schema, StateKey, Value};
use crate::update::Update;
use crate::value::Key;

/// Where a value a node gives goes in the state of the graph it runs in, by
/// name: an output of a called graph (`SubGraph`), or a text a node gives as
/// a description writes it (`TextOutput`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// Sets a key of the value's kind.
    Set(Key),
    /// Appends to a list of the value's kind.
    Append(Key),
}

impl Output {
    /// The key written.
    pub fn key(&self) -> &Key {
        match self {
            Output::Set(key) | Output::Append(key) => key,
        }
    }

    pub(crate) fn update(&self, value: Value) -> Update {
        match self {
            Output::Set(key) => Update::Set {
                key: key.clone(),
                value,
            },
            Output::Append(key) => Update::Append {
                key: key.clone(),
                value,
            },
        }
    }
}

/// Where a node writes a text it gives (the answer of an `LlmCall`, the
/// answer or the thread of a `RunnerCall`): a string state key it sets, or a
/// list of strings it appends to.
///
/// More outputs may come in a later version: a `match` on it outside the
/// crate needs a wildcard arm.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TextOutput {
    /// Sets a string state key to the text.
    Set(StateKey<String>),
    /// Appends the text to a list of strings.
    Append(StateKey<Vec<String>>),
}

impl TextOutput {
    /// The key the text goes to.
    pub fn key(&self) -> &Key {
        match self {
            TextOutput::Set(key) => key.key(),
            TextOutput::Append(key) => key.key(),
        }
    }

    /// Where it writes, by name, as a description writes it.
    pub(crate) fn written(&self) -> Output {
        match self {
            TextOutput::Set(key) => Output::Set(key.key().clone()),
            TextOutput::Append(key) => Output::Append(key.key().clone()),
        }
    }

    /// The update that writes `text` through it.
    pub(crate) fn update(&self, text: String) -> Update {
        self.written().update(Value::str(text))
    }

    /// The build check of a node that writes a text through it: the key must
    /// take a string, refused with the error applying one would give
    /// otherwise (`UnknownKey`, `KindMismatch`, `AppendNotList`). In a map
    /// body, a `Set` is refused (`MapBodySet`).
    pub(crate) fn check(&self, schema: &Schema, site: CheckSite) -> Result<(), GraphError> {
        let key = self.key();
        let expected = match (self, schema.state.get(key)) {
            (TextOutput::Set(_), _) if site == CheckSite::MapBody => {
                return Err(GraphError::MapBodySet { key: key.clone() });
            }
            (_, None) => return Err(GraphError::UnknownKey { key: key.clone() }),
            (TextOutput::Set(_), Some(Kind::Str)) => return Ok(()),
            (TextOutput::Set(_), Some(kind)) => kind,
            (TextOutput::Append(_), Some(Kind::List { element })) if **element == Kind::Str => {
                return Ok(());
            }
            (TextOutput::Append(_), Some(Kind::List { element })) => element.as_ref(),
            (TextOutput::Append(_), Some(_)) => {
                return Err(GraphError::AppendNotList { key: key.clone() });
            }
        };
        Err(GraphError::KindMismatch {
            key: key.clone(),
            expected: expected.clone(),
            found: Kind::Str.tag(),
        })
    }
}
