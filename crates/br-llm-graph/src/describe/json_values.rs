//! The values a description writes: sources and targets of mappings,
//! captures, bounds, round limits and authors.

use serde::ser::{Serialize, SerializeMap, Serializer};

use crate::describe::shape::{CaptureShape, Source};
use crate::graph::{Count, Limit, Output};
use crate::react::{OnLimit, Origin, RoundLimit};
use crate::state::Kind;
use crate::value::{Key, NodeId};

#[derive(serde::Serialize)]
pub(crate) struct Mapping<'a> {
    pub(crate) child: &'a Key,
    pub(crate) source: SourceJson<'a>,
}

#[derive(serde::Serialize)]
pub(crate) struct OutputMapping<'a> {
    pub(crate) child: &'a Key,
    pub(crate) target: Written<'a>,
}

#[derive(serde::Serialize)]
pub(crate) struct CaptureJson<'a> {
    pub(crate) target: Written<'a>,
    pub(crate) source: SourceJson<'a>,
}

/// `{"capture": [{"target", "source"}]}`.
pub(crate) struct Capturing<'a>(pub(crate) &'a [CaptureShape]);

impl Serialize for Capturing<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let captures: Vec<CaptureJson<'_>> = self
            .0
            .iter()
            .map(|capture| CaptureJson {
                target: written(&capture.target),
                source: capture.source.json(),
            })
            .collect();
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry("capture", &captures)?;
        map.end()
    }
}

/// `{"from": key}`, `{"config": key}`, `{"const": kind}` or
/// `{"reason": true}`.
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceJson<'a> {
    From(&'a Key),
    Config(&'a Key),
    Const(ConstKind<'a>),
    Reason(bool),
}

/// The kind of a fixed value; an empty list, whose element kind it does
/// not say, is `{"type": "list"}`.
#[derive(serde::Serialize)]
#[serde(untagged)]
pub(crate) enum ConstKind<'a> {
    Known(&'a Kind),
    List {
        #[serde(rename = "type")]
        tag: &'static str,
    },
}

impl Source {
    pub(crate) fn json(&self) -> SourceJson<'_> {
        match self {
            Source::From(key) => SourceJson::From(key),
            Source::Config(key) => SourceJson::Config(key),
            Source::Const(Some(kind)) => SourceJson::Const(ConstKind::Known(kind)),
            Source::Const(None) => SourceJson::Const(ConstKind::List { tag: "list" }),
            Source::Reason => SourceJson::Reason(true),
        }
    }
}

/// `{"set": key}` or `{"append": key}`.
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Written<'a> {
    Set(&'a Key),
    Append(&'a Key),
}

pub(crate) fn written(output: &Output) -> Written<'_> {
    match output {
        Output::Set(key) => Written::Set(key),
        Output::Append(key) => Written::Append(key),
    }
}

/// `{"fixed": n}` or `{"config": key}`, for a `Limit` or a `Count`.
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Bound<'a> {
    Fixed(usize),
    Config(&'a Key),
}

pub(crate) fn limit_json(limit: &Limit) -> Bound<'_> {
    match limit {
        Limit::Fixed(value) => Bound::Fixed(value.get()),
        Limit::Config(key) => Bound::Config(key),
    }
}

pub(crate) fn count_json(count: &Count) -> Bound<'_> {
    match count {
        Count::Fixed(value) => Bound::Fixed(*value),
        Count::Config(key) => Bound::Config(key),
    }
}

#[derive(serde::Serialize)]
pub(crate) struct RoundLimitJson<'a> {
    max_rounds: Bound<'a>,
    on_limit: OnLimitJson<'a>,
}

/// `"error"`, `{"continue": {"node", "flag"}}` or `{"end": {"node", "flag"}}`.
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OnLimitJson<'a> {
    Error,
    Continue { node: &'a NodeId, flag: &'a Key },
    End { node: &'a NodeId, flag: &'a Key },
}

pub(crate) fn round_limit_json(round_limit: &RoundLimit) -> RoundLimitJson<'_> {
    RoundLimitJson {
        max_rounds: limit_json(&round_limit.max_rounds),
        on_limit: match &round_limit.on_limit {
            OnLimit::Error => OnLimitJson::Error,
            OnLimit::Continue { node, flag } => OnLimitJson::Continue { node, flag },
            OnLimit::End { node, flag } => OnLimitJson::End { node, flag },
        },
    }
}

#[derive(serde::Serialize)]
/// `{"id"}`: an author by its id, never its name.
pub(crate) struct Author<'a> {
    pub(crate) id: &'a str,
}

#[derive(serde::Serialize)]
pub(crate) struct Text {
    pub(crate) text: Origin,
}

#[derive(serde::Serialize)]
pub(crate) struct History {
    pub(crate) history: Origin,
}

#[derive(serde::Serialize)]
pub(crate) struct Conversation<'a> {
    pub(crate) conversation: &'a Key,
    pub(crate) author: Author<'a>,
}

/// `"fixed"`, `"ready_made"` or `"function"`.
impl Serialize for Origin {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
