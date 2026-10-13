use std::sync::Arc;

use br_llm_messages::Step;
use serde_json::{Map, Value as Json, json};

use crate::error::GraphError;
use crate::react::helpers::structured;
use crate::react::llm_node::Source;
use crate::state::{Config, State};

/// The critic's judgement of one answer. `message` may be blank (empty or
/// whitespace only) only when `is_valid` is true.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Verdict {
    pub is_valid: bool,
    pub message: String,
}

/// Reads a verdict from the critic's structured block: pure and synchronous.
pub type VerdictReader = Arc<dyn Fn(&Json) -> Result<Verdict, GraphError> + Send + Sync>;

/// A verdict schema of the caller's, sent instead of the default one.
///
/// Without a `reader`, the block is read as the default verdict, so the build
/// checks the schema: an object whose properties include `is_valid`
/// (boolean) and `message` (string), both required
/// (`VerdictSchemaMismatch`). With a `reader`, the schema is the caller's
/// concern and the reader gives the verdict.
pub struct VerdictOverride {
    pub schema: Json,
    pub reader: Option<VerdictReader>,
}

/// The default description of `thinking`.
const THINKING: &str = "Explanation of the thought process and validation steps";
/// The default description of `is_valid`.
const IS_VALID: &str = "Boolean indicating if the generated message is valid";
/// The default description of `message`.
const MESSAGE: &str = "Feedback message for the generator, empty if validation passed";

/// How the critic's verdict is asked for and read.
pub(crate) enum VerdictForm {
    Default {
        reasoning_field: bool,
        thinking: Source,
        is_valid: Source,
        message: Source,
    },
    Override(VerdictOverride),
}

impl VerdictForm {
    /// The default form, each description a source or its default text.
    pub(crate) fn default_form(
        reasoning_field: bool,
        thinking: Option<Source>,
        is_valid: Option<Source>,
        message: Option<Source>,
    ) -> Self {
        let or = |source: Option<Source>, text: &str| {
            source.unwrap_or_else(|| Source::Const(text.to_owned()))
        };
        VerdictForm::Default {
            reasoning_field,
            thinking: or(thinking, THINKING),
            is_valid: or(is_valid, IS_VALID),
            message: or(message, MESSAGE),
        }
    }

    /// The sources this form reads.
    pub(crate) fn sources(&self) -> Vec<&Source> {
        match self {
            VerdictForm::Default {
                reasoning_field,
                thinking,
                is_valid,
                message,
            } => {
                let mut sources = vec![is_valid, message];
                if *reasoning_field {
                    sources.insert(0, thinking);
                }
                sources
            }
            VerdictForm::Override(_) => Vec::new(),
        }
    }

    /// Refuses an override without a reader whose schema cannot be read as
    /// the default verdict.
    pub(crate) fn check(&self) -> Result<(), GraphError> {
        match self {
            VerdictForm::Override(VerdictOverride {
                schema,
                reader: None,
            }) => check_schema(schema),
            VerdictForm::Override(_) | VerdictForm::Default { .. } => Ok(()),
        }
    }

    /// The JSON Schema sent to the critic on this call.
    pub(crate) fn schema(&self, state: &State, config: &Config) -> Result<Json, GraphError> {
        let (reasoning_field, thinking, is_valid, message) = match self {
            VerdictForm::Override(VerdictOverride { schema, .. }) => return Ok(schema.clone()),
            VerdictForm::Default {
                reasoning_field,
                thinking,
                is_valid,
                message,
            } => (*reasoning_field, thinking, is_valid, message),
        };
        let mut properties = Map::new();
        if reasoning_field {
            properties.insert(
                "thinking".to_owned(),
                json!({ "type": "string", "description": thinking.resolve(state, config)? }),
            );
        }
        properties.insert(
            "is_valid".to_owned(),
            json!({ "type": "boolean", "description": is_valid.resolve(state, config)? }),
        );
        properties.insert(
            "message".to_owned(),
            json!({ "type": "string", "description": message.resolve(state, config)? }),
        );
        let required: Vec<&String> = properties.keys().collect();
        Ok(json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        }))
    }

    /// Reads the verdict of the critic's step. A rejection with a blank
    /// message (empty or whitespace only) is refused (`Structured`).
    pub(crate) fn read(&self, step: &Step) -> Result<Verdict, GraphError> {
        let verdict = match self {
            VerdictForm::Override(VerdictOverride {
                reader: Some(reader),
                ..
            }) => {
                let value = step
                    .structured()
                    .next()
                    .ok_or_else(|| GraphError::Structured {
                        message: "the step carries no structured block".to_owned(),
                    })?;
                reader(value)?
            }
            VerdictForm::Override(_) | VerdictForm::Default { .. } => structured(step)?,
        };
        if !verdict.is_valid && verdict.message.trim().is_empty() {
            return Err(GraphError::Structured {
                message: "a rejecting verdict carries a blank message".to_owned(),
            });
        }
        Ok(verdict)
    }
}

fn check_schema(schema: &Json) -> Result<(), GraphError> {
    let refuse = |reason| Err(GraphError::VerdictSchemaMismatch { reason });
    if schema.get("type") != Some(&json!("object")) {
        return refuse("the schema is not an object");
    }
    let Some(properties) = schema.get("properties").and_then(Json::as_object) else {
        return refuse("the schema has no properties");
    };
    if properties.get("is_valid").and_then(|p| p.get("type")) != Some(&json!("boolean")) {
        return refuse("is_valid is not a boolean property");
    }
    if properties.get("message").and_then(|p| p.get("type")) != Some(&json!("string")) {
        return refuse("message is not a string property");
    }
    let required = schema
        .get("required")
        .and_then(Json::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    for name in ["is_valid", "message"] {
        if !required.contains(&json!(name)) {
            return refuse("is_valid and message are not both required");
        }
    }
    Ok(())
}
