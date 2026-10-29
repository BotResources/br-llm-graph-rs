use br_llm_clients_core::{Parameter, ParameterKind, Parameters, StructuredShape};
use br_llm_messages::{ContentBlock, ModelResponse};
use serde::Deserialize;

use crate::fault::Fault;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Answer {
    pub(crate) validated: bool,
    pub(crate) message: String,
}

#[derive(Deserialize)]
struct Wire {
    is_validated: bool,
    #[serde(default)]
    message: Option<String>,
}

pub(crate) fn unwrap_answer(response: &ModelResponse) -> Result<Answer, String> {
    let structured = response
        .blocks()
        .iter()
        .find_map(|block| match block {
            ContentBlock::StructuredContent(structured) => Some(structured),
            _ => None,
        })
        .ok_or_else(|| "the response holds no structured answer".to_owned())?;
    let wire: Wire = structured
        .value()
        .deserialize_into()
        .map_err(|error| error.to_string())?;
    if wire.is_validated {
        return Ok(Answer {
            validated: true,
            message: wire.message.unwrap_or_default(),
        });
    }
    match wire.message {
        Some(message) if !message.trim().is_empty() => Ok(Answer {
            validated: false,
            message,
        }),
        _ => Err("the critic does not validate and gives a blank message".to_owned()),
    }
}

pub(crate) fn critic_shape(thinking_field: bool) -> Result<StructuredShape, Fault> {
    let mut list = Vec::new();
    if thinking_field {
        list.push(
            Parameter::new(
                "thinking",
                "Think about the work before you judge it.",
                true,
                ParameterKind::String,
            )
            .map_err(Fault::value)?,
        );
    }
    list.push(
        Parameter::new(
            "is_validated",
            "True when the work is good enough and nothing more is needed.",
            true,
            ParameterKind::Boolean,
        )
        .map_err(Fault::value)?,
    );
    list.push(
        Parameter::new(
            "message",
            "What the generator must do to improve its work. Not blank when the work is not validated.",
            true,
            ParameterKind::String,
        )
        .map_err(Fault::value)?,
    );
    StructuredShape::new(
        "critic_answer",
        Some("The judgement of the critic on the work of the generator.".to_owned()),
        Parameters::new(list).map_err(Fault::value)?,
    )
    .map_err(Fault::value)
}
