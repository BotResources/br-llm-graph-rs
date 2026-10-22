//! The fields each kind of node writes in a description.

use serde::ser::SerializeMap;

use crate::describe::json::Nested;
use crate::describe::json_values::{
    Author, Capturing, Conversation, History, Mapping, OutputMapping, Text, count_json, limit_json,
    round_limit_json, written,
};
use crate::describe::shape::{
    CallShape, CheckShape, CriticShape, ItemPolicy, LlmCallShape, MapShape, ModelCallShape,
    RepliesShape, RequestShape, RunnerCallShape, Source,
};
use crate::value::Key;

pub(crate) fn call_fields<M: SerializeMap>(call: &CallShape, map: &mut M) -> Result<(), M::Error> {
    if !call.inputs.is_empty() {
        map.serialize_entry("inputs", &mappings(&call.inputs))?;
    }
    if !call.config.is_empty() {
        map.serialize_entry("config", &mappings(&call.config))?;
    }
    if !call.outputs.is_empty() {
        let outputs: Vec<OutputMapping<'_>> = call
            .outputs
            .iter()
            .map(|(child, target)| OutputMapping {
                child,
                target: written(target),
            })
            .collect();
        map.serialize_entry("outputs", &outputs)?;
    }
    if let Some(target) = &call.end_label {
        map.serialize_entry("end_label", &written(target))?;
    }
    match &call.captures {
        None => map.serialize_entry("on_failure", "propagate")?,
        Some(captures) => map.serialize_entry("on_failure", &Capturing(captures))?,
    }
    map.serialize_entry("graph", &Nested(&call.graph))
}

fn mappings(mappings: &[(Key, Source)]) -> Vec<Mapping<'_>> {
    mappings
        .iter()
        .map(|(child, source)| Mapping {
            child,
            source: source.json(),
        })
        .collect()
}

pub(crate) fn map_fields<M: SerializeMap>(shape: &MapShape, map: &mut M) -> Result<(), M::Error> {
    map.serialize_entry("list", &shape.list)?;
    map.serialize_entry("item", &shape.item)?;
    if let Some(limit) = &shape.max_concurrency {
        map.serialize_entry("max_concurrency", &limit_json(limit))?;
    }
    match &shape.on_item_failure {
        ItemPolicy::Finish => map.serialize_entry("on_item_failure", "finish")?,
        ItemPolicy::FailFast => map.serialize_entry("on_item_failure", "fail_fast")?,
        ItemPolicy::Capture(captures) => {
            map.serialize_entry("on_item_failure", &Capturing(captures))?
        }
    }
    map.serialize_entry("body", &shape.body)
}

pub(crate) fn llm_call_fields<M: SerializeMap>(
    call: &LlmCallShape,
    map: &mut M,
) -> Result<(), M::Error> {
    map.serialize_entry("model", call.model.as_str())?;
    map.serialize_entry("system", call.system.as_str())?;
    match &call.request {
        RequestShape::Text(origin) => map.serialize_entry("request", &Text { text: *origin })?,
        RequestShape::History(origin) => {
            map.serialize_entry("request", &History { history: *origin })?
        }
        RequestShape::Conversation { key, author } => map.serialize_entry(
            "request",
            &Conversation {
                conversation: key,
                author: Author { id: &author.id },
            },
        )?,
    }
    map.serialize_entry("output", &written(&call.output))?;
    replies_fields(&call.replies, map)
}

fn replies_fields<M: SerializeMap>(replies: &RepliesShape, map: &mut M) -> Result<(), M::Error> {
    map.serialize_entry("reply_retries", &count_json(&replies.retries))?;
    map.serialize_entry("reminder", replies.reminder.as_str())
}

pub(crate) fn model_call_fields<M: SerializeMap>(
    call: &ModelCallShape,
    map: &mut M,
) -> Result<(), M::Error> {
    let author = Author {
        id: &call.author.id,
    };
    map.serialize_entry("author", &author)?;
    map.serialize_entry("model", call.model.as_str())?;
    map.serialize_entry("system", call.system.as_str())?;
    map.serialize_entry("history", call.history.as_str())?;
    if !call.tools.is_empty() {
        map.serialize_entry("tools", &call.tools)?;
    }
    if let Some(key) = &call.enabled {
        map.serialize_entry("enabled", key)?;
    }
    if let Some(limit) = &call.tool_concurrency {
        map.serialize_entry("tool_concurrency", &limit_json(limit))?;
    }
    if let Some(round_limit) = &call.round_limit {
        map.serialize_entry("round_limit", &round_limit_json(round_limit))?;
    }
    if let Some(replies) = &call.replies {
        replies_fields(replies, map)?;
    }
    if let Some(critic) = &call.critic {
        critic_fields(critic, map)?;
    }
    Ok(())
}

fn critic_fields<M: SerializeMap>(critic: &CriticShape, map: &mut M) -> Result<(), M::Error> {
    map.serialize_entry("max_critiques", &count_json(&critic.max_critiques))?;
    map.serialize_entry("final_generation", &critic.final_generation)?;
    map.serialize_entry("verdict_retries", &count_json(&critic.verdict_retries))?;
    map.serialize_entry("review_request", critic.review_request.as_str())?;
    let verdict = if critic.verdict_override {
        "override"
    } else {
        "default"
    };
    map.serialize_entry("verdict", verdict)
}

pub(crate) fn check_fields<M: SerializeMap>(
    check: &CheckShape,
    map: &mut M,
) -> Result<(), M::Error> {
    map.serialize_entry("revision_limit", &count_json(&check.revision_limit))?;
    if let Some(origin) = check.rule_check {
        map.serialize_entry("rule_check", origin.as_str())?;
    }
    map.serialize_entry(
        "empty_answer_critique",
        check.empty_answer_critique.as_str(),
    )
}

pub(crate) fn runner_call_fields<M: SerializeMap>(
    call: &RunnerCallShape,
    map: &mut M,
) -> Result<(), M::Error> {
    map.serialize_entry("target", &call.target)?;
    map.serialize_entry("arguments", call.arguments.as_str())?;
    map.serialize_entry("output", &written(&call.output))?;
    if let Some(thread) = &call.thread {
        map.serialize_entry("thread", &written(thread))?;
    }
    Ok(())
}
