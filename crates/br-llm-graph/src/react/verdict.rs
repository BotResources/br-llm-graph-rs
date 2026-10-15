use std::sync::Arc;

use br_llm_clients_core::{OutputMode, Parameter, ParameterKind, Parameters, StructuredShape};
use br_llm_messages::Step;
use serde_json::Value as Json;

use crate::error::GraphError;
use crate::graph::NodeError;
use crate::react::given::{Prompt, Reads};
use crate::react::helpers::structured;
use crate::react::seats::CriticSeat;
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

/// A verdict shape of the caller's, sent instead of the default one; the
/// descriptions of the critic seat are then ignored.
///
/// Without a `reader`, the block is read as the default verdict, so the build
/// checks the shape: its parameters include `is_valid` (boolean) and
/// `message` (string), both required (`VerdictShapeMismatch`). With a
/// `reader`, the shape is the caller's concern and the reader gives the
/// verdict.
pub struct VerdictOverride {
    pub shape: StructuredShape,
    pub reader: Option<VerdictReader>,
}

/// The name of the default verdict shape.
const SHAPE: &str = "verdict";
/// The default description of `thinking`.
const THINKING: &str = "Explanation of the thought process and validation steps";
/// The default description of `is_valid`.
const IS_VALID: &str = "Boolean indicating if the generated message is valid";
/// The default description of `message`.
const MESSAGE: &str = "Feedback message for the generator, empty if validation passed";

/// How the critic's verdict is asked for and read.
pub(crate) enum VerdictForm {
    /// The default shape, `verdict`. `thinking` is the description of the
    /// `thinking` parameter, `None` when the shape has none (a critic that
    /// reasons natively).
    Default {
        thinking: Option<Prompt<CriticSeat>>,
        is_valid: Prompt<CriticSeat>,
        message: Prompt<CriticSeat>,
    },
    Override(VerdictOverride),
}

impl VerdictForm {
    /// The default form, each description the given text or its default. The
    /// shape has `thinking` exactly when `with_thinking` (the critic's client
    /// does not reason: its `ReasoningLevel` is `Off`).
    pub(crate) fn default_form(
        with_thinking: bool,
        thinking: Option<Prompt<CriticSeat>>,
        is_valid: Option<Prompt<CriticSeat>>,
        message: Option<Prompt<CriticSeat>>,
    ) -> Self {
        let or = |given: Option<Prompt<CriticSeat>>, text: &str| {
            given.unwrap_or_else(|| Prompt::text(text))
        };
        VerdictForm::Default {
            thinking: with_thinking.then(|| or(thinking, THINKING)),
            is_valid: or(is_valid, IS_VALID),
            message: or(message, MESSAGE),
        }
    }

    /// The keys the descriptions in use read.
    pub(crate) fn reads(&self) -> Vec<&Reads> {
        match self {
            VerdictForm::Default {
                thinking,
                is_valid,
                message,
            } => thinking
                .iter()
                .chain([is_valid, message])
                .map(Prompt::reads)
                .collect(),
            VerdictForm::Override(_) => Vec::new(),
        }
    }

    /// Refuses an override without a reader whose shape cannot be read as
    /// the default verdict (`VerdictShapeMismatch`).
    pub(crate) fn check(&self) -> Result<(), GraphError> {
        match self {
            VerdictForm::Override(VerdictOverride {
                shape,
                reader: None,
            }) => check_shape(shape),
            VerdictForm::Override(_) | VerdictForm::Default { .. } => Ok(()),
        }
    }

    /// The output mode sent to the critic on this call: the structured mode
    /// with the verdict shape, its descriptions resolved now.
    pub(crate) fn output_mode(
        &self,
        state: &State,
        config: &Config,
    ) -> Result<OutputMode, NodeError> {
        let (thinking, is_valid, message) = match self {
            VerdictForm::Override(VerdictOverride { shape, .. }) => {
                return Ok(OutputMode::Structured(shape.clone()));
            }
            VerdictForm::Default {
                thinking,
                is_valid,
                message,
            } => (thinking, is_valid, message),
        };
        let parameter = |name: &str, description: &Prompt<CriticSeat>, kind: ParameterKind| {
            Parameter::new(name, description.give(state, config, ())?, true, kind)
                .map_err(|error| NodeError::from(GraphError::Request(error)))
        };
        let mut parameters = Vec::new();
        if let Some(thinking) = thinking {
            parameters.push(parameter("thinking", thinking, ParameterKind::String)?);
        }
        parameters.push(parameter("is_valid", is_valid, ParameterKind::Boolean)?);
        parameters.push(parameter("message", message, ParameterKind::String)?);
        let parameters = Parameters::new(parameters).map_err(GraphError::Request)?;
        let shape = StructuredShape::new(SHAPE, None, parameters).map_err(GraphError::Request)?;
        Ok(OutputMode::Structured(shape))
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

fn check_shape(shape: &StructuredShape) -> Result<(), GraphError> {
    let find = |name: &str, kind: ParameterKind| {
        shape
            .parameters()
            .iter()
            .find(|parameter| parameter.name() == name && *parameter.kind() == kind)
    };
    let Some(is_valid) = find("is_valid", ParameterKind::Boolean) else {
        return Err(GraphError::VerdictShapeMismatch {
            reason: "is_valid is not a boolean parameter",
        });
    };
    let Some(message) = find("message", ParameterKind::String) else {
        return Err(GraphError::VerdictShapeMismatch {
            reason: "message is not a string parameter",
        });
    };
    if !is_valid.is_required() || !message.is_required() {
        return Err(GraphError::VerdictShapeMismatch {
            reason: "is_valid and message are not both required",
        });
    }
    Ok(())
}
