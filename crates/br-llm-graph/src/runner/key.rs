//! The call key: what names one runner call across attempts.

use br_llm_messages::ToolCallId;
use serde_json::{Map, Value};

use crate::origin::Origin;

/// What names one runner call across attempts: the origin of the node that
/// makes it and its arguments, and, for a runner tool, the id of the tool
/// call.
///
/// - `origin`: the run id and the occurrence of the node that makes the
///   call: the runner call node (an item of a map as `m[3]`, a node of a
///   called graph as `call/x`), or the tool node that runs a runner tool.
/// - `arguments`: the call's arguments as their canonical JSON text, so
///   equal arguments give one text whatever the order they were built in.
///   Canonical here means: no whitespace; the names of every object sorted
///   by their UTF-8 bytes; strings escaped and numbers written as serde_json
///   writes them, so `1` and `1.0` are two keys. It is not RFC 8785 (JCS),
///   which sorts by UTF-16 code units and writes numbers otherwise.
/// - `tool_call`: the id of the tool call a runner tool answers; `None` for
///   a runner call node.
///
/// The same key names the same call. A resumed run keeps its run id and
/// rebuilds the same arguments, so it gives the keys it gave before, and the
/// port finds the call it had started instead of starting another. A node
/// that a loop of its graph runs again with the same arguments names the
/// same call and gets the same answer; with other arguments, it makes
/// another call. The port derives from the key whatever it creates (the
/// child's thread, its job, the id of its answer): `arguments` can be large
/// and hold sensitive values (a passport, a secret), so a port hashes it,
/// with the rest of the key, before it derives an id, rather than writing it
/// into one. A host that wants a new attempt of its job to find the calls
/// of the earlier one gives that attempt the same run id.
///
/// Its fields are read by the host's port and built with `new`: it may gain
/// fields in a later version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct CallKey {
    /// The run id and the occurrence of the node that makes the call.
    pub origin: Origin,
    /// The call's arguments, as their canonical JSON text.
    pub arguments: String,
    /// The id of the tool call, for a runner tool.
    pub tool_call: Option<ToolCallId>,
}

impl CallKey {
    /// The key of the call the node at `origin` makes with `arguments`,
    /// answering the tool call `tool_call` for a runner tool.
    pub fn new(
        origin: Origin,
        arguments: &Map<String, Value>,
        tool_call: Option<ToolCallId>,
    ) -> Self {
        let mut text = String::new();
        canonical_object(arguments, &mut text);
        Self {
            origin,
            arguments: text,
            tool_call,
        }
    }
}

/// Writes `value` as canonical JSON: no whitespace, object names sorted.
fn canonical(value: &Value, text: &mut String) {
    match value {
        Value::Object(object) => canonical_object(object, text),
        Value::Array(items) => {
            text.push('[');
            for (position, item) in items.iter().enumerate() {
                if position > 0 {
                    text.push(',');
                }
                canonical(item, text);
            }
            text.push(']');
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            text.push_str(&value.to_string());
        }
    }
}

fn canonical_object(object: &Map<String, Value>, text: &mut String) {
    let mut entries: Vec<(&String, &Value)> = object.iter().collect();
    entries.sort_by_key(|(name, _)| *name);
    text.push('{');
    for (position, (name, value)) in entries.into_iter().enumerate() {
        if position > 0 {
            text.push(',');
        }
        text.push_str(&Value::from(name.as_str()).to_string());
        text.push(':');
        canonical(value, text);
    }
    text.push('}');
}

#[cfg(test)]
#[path = "key_tests.rs"]
mod tests;
