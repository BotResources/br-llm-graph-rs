//! Whether a structured value fits its shape: an object that holds every
//! required parameter and no name the shape does not declare, each value of
//! its parameter's kind. Pure, over the types of br-llm-clients-core alone,
//! so it can move there: br-llm-clients-core does not check a structured
//! reply against its shape.

use std::fmt;

use br_llm_clients_core::{ParameterKind, Parameters, StructuredShape};
use serde_json::Value as Json;

/// Checks `value` against `shape`: `Ok` when it fits, else where it does
/// not, and how.
///
/// A value fits when it is an object that holds every required parameter
/// and no name the shape does not declare, each value of its parameter's
/// kind: a string, a number, an integer (a JSON integer: `1.0` is not one),
/// a boolean, a string among the values of an enum, an array whose items
/// fit, or an object that fits its parameters. A `null` member holds
/// nothing: a required parameter given `null` is missing, an optional one is
/// absent. The parameters are checked in the order the shape declares them,
/// then the names it does not declare, and the first misfit is given.
pub(crate) fn fit(shape: &StructuredShape, value: &Json) -> Result<(), Misfit> {
    object(shape.parameters(), value, &Path::default())
}

/// Where a structured value does not fit its shape, and how. Its display is
/// the reason: it names the place (`answer`, `sources[2].url`) and the kind
/// asked for, never the value found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Misfit {
    at: Path,
    wrong: Wrong,
}

/// How a value does not fit.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Wrong {
    /// A required parameter the object does not hold.
    Missing,
    /// A name the shape does not declare.
    Undeclared,
    /// A value that is not of its parameter's kind.
    Not(Expected),
}

/// The kind a value is not.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Expected {
    String,
    Number,
    Integer,
    Boolean,
    OneOf(Vec<String>),
    Array,
    Object,
}

/// The place of a value in the structured value: the names of the members
/// and the indexes of the items that lead to it, empty for the value itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Path(Vec<Segment>);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Name(String),
    Index(usize),
}

impl Path {
    fn then(&self, segment: Segment) -> Path {
        let mut path = self.0.clone();
        path.push(segment);
        Path(path)
    }

    fn misfit(&self, wrong: Wrong) -> Misfit {
        Misfit {
            at: self.clone(),
            wrong,
        }
    }
}

/// `value` against `parameters`, the value at `at`.
fn object(parameters: &Parameters, value: &Json, at: &Path) -> Result<(), Misfit> {
    let Some(members) = value.as_object() else {
        return Err(at.misfit(Wrong::Not(Expected::Object)));
    };
    for parameter in parameters {
        let place = at.then(Segment::Name(parameter.name().to_owned()));
        match members.get(parameter.name()) {
            None | Some(Json::Null) if parameter.is_required() => {
                return Err(place.misfit(Wrong::Missing));
            }
            None | Some(Json::Null) => {}
            Some(found) => kind(parameter.kind(), found, &place)?,
        }
    }
    match members
        .keys()
        .find(|name| !parameters.iter().any(|parameter| parameter.name() == *name))
    {
        Some(name) => Err(at
            .then(Segment::Name(name.clone()))
            .misfit(Wrong::Undeclared)),
        None => Ok(()),
    }
}

/// `value` against `kind`, the value at `at`.
fn kind(kind: &ParameterKind, value: &Json, at: &Path) -> Result<(), Misfit> {
    let (fits, expected) = match kind {
        ParameterKind::String => (value.is_string(), Expected::String),
        ParameterKind::Number => (value.is_number(), Expected::Number),
        ParameterKind::Integer => (value.is_i64() || value.is_u64(), Expected::Integer),
        ParameterKind::Boolean => (value.is_boolean(), Expected::Boolean),
        ParameterKind::Enum { values } => (
            value
                .as_str()
                .is_some_and(|text| values.iter().any(|allowed| allowed == text)),
            Expected::OneOf(values.clone()),
        ),
        ParameterKind::Array { items } => {
            let Some(elements) = value.as_array() else {
                return Err(at.misfit(Wrong::Not(Expected::Array)));
            };
            for (index, element) in elements.iter().enumerate() {
                self::kind(items, element, &at.then(Segment::Index(index)))?;
            }
            return Ok(());
        }
        ParameterKind::Object { parameters } => return object(parameters, value, at),
        // A kind br-llm-clients-core 0.4 does not have: the graph pins that
        // version, so none reaches it; one a later version adds is not
        // checked until the check knows it.
        _ => return Ok(()),
    };
    if fits {
        Ok(())
    } else {
        Err(at.misfit(Wrong::Not(expected)))
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (position, segment) in self.0.iter().enumerate() {
            match segment {
                Segment::Name(name) if position == 0 => f.write_str(name)?,
                Segment::Name(name) => write!(f, ".{name}")?,
                Segment::Index(index) => write!(f, "[{index}]")?,
            }
        }
        Ok(())
    }
}

impl fmt::Display for Expected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expected::String => f.write_str("a string"),
            Expected::Number => f.write_str("a number"),
            Expected::Integer => f.write_str("an integer"),
            Expected::Boolean => f.write_str("a boolean"),
            Expected::OneOf(values) => {
                f.write_str("one of ")?;
                for (position, value) in values.iter().enumerate() {
                    if position > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "`{value}`")?;
                }
                Ok(())
            }
            Expected::Array => f.write_str("an array"),
            Expected::Object => f.write_str("an object"),
        }
    }
}

impl fmt::Display for Misfit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.at.0.is_empty() {
            f.write_str("the value")?;
        } else {
            write!(f, "`{}`", self.at)?;
        }
        match &self.wrong {
            Wrong::Missing => f.write_str(" is missing"),
            Wrong::Undeclared => f.write_str(" is not in the shape"),
            Wrong::Not(expected) => write!(f, " is not {expected}"),
        }
    }
}

#[cfg(test)]
#[path = "fit_tests.rs"]
mod tests;
