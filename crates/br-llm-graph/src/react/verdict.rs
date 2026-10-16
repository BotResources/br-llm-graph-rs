use std::sync::Arc;

use br_llm_clients_core::{OutputMode, Parameter, ParameterKind, Parameters, StructuredShape};
use br_llm_messages::Step;
use serde_json::Value as Json;

use crate::error::GraphError;
use crate::graph::NodeError;
use crate::react::given::{CriticFacts, Prompt, Reads};
use crate::react::seats::CriticSeat;
use crate::state::{Config, State};

/// The critic's judgment of one answer. `message` may be blank (empty or
/// whitespace only) only when `is_valid` is true.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Verdict {
    /// Whether the answer is valid.
    pub is_valid: bool,
    /// The feedback: what the generator reads in a critique, what an
    /// acceptance holds.
    pub message: String,
}

/// Reads a verdict from the critic's structured block: pure and synchronous.
/// The verdict's `message` is what the generator reads in a critique, or
/// what an acceptance holds. Its error fails the run as returned, without a
/// retry; a rejection it gives with a blank message is an unreadable
/// verdict, asked again while verdict retries are left.
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
    /// The shape sent instead of the default one.
    pub shape: StructuredShape,
    /// The reader of the structured block. `None`: the block is read as the
    /// default verdict.
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
    /// The default shape, `verdict`, with the description of each parameter.
    /// The shape has `thinking` exactly when the client of the call does not
    /// reason (its `ReasoningLevel` is `Off`), known at each call.
    Default {
        thinking: Prompt<CriticSeat>,
        is_valid: Prompt<CriticSeat>,
        message: Prompt<CriticSeat>,
    },
    Override(VerdictOverride),
}

/// What reading the critic's step gives, short of a reader's error: a
/// verdict, or the reason it is unreadable.
pub(crate) enum Reading {
    Verdict(Verdict),
    Unreadable(String),
}

impl VerdictForm {
    /// The form of the critic: `verdict`, else the default shape, each
    /// description the given text or its default.
    pub(crate) fn new(
        verdict: Option<VerdictOverride>,
        thinking: Option<Prompt<CriticSeat>>,
        is_valid: Option<Prompt<CriticSeat>>,
        message: Option<Prompt<CriticSeat>>,
    ) -> Self {
        if let Some(custom) = verdict {
            return VerdictForm::Override(custom);
        }
        let or = |given: Option<Prompt<CriticSeat>>, text: &str| {
            given.unwrap_or_else(|| Prompt::text(text))
        };
        VerdictForm::Default {
            thinking: or(thinking, THINKING),
            is_valid: or(is_valid, IS_VALID),
            message: or(message, MESSAGE),
        }
    }

    /// The keys the descriptions read: all three for the default shape,
    /// since whether a call asks for `thinking` is known only at the call.
    pub(crate) fn reads(&self) -> Vec<&Reads> {
        match self {
            VerdictForm::Default {
                thinking,
                is_valid,
                message,
            } => [thinking, is_valid, message]
                .into_iter()
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
    /// with the verdict shape, its descriptions given now for the critic's
    /// context `facts`, with `thinking` when `with_thinking`.
    pub(crate) fn output_mode(
        &self,
        state: &State,
        config: &Config,
        facts: CriticFacts<'_>,
        with_thinking: bool,
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
            Parameter::new(name, description.give(state, config, facts)?, true, kind)
                .map_err(|error| NodeError::from(GraphError::Request(error)))
        };
        let mut parameters = Vec::new();
        if with_thinking {
            parameters.push(parameter("thinking", thinking, ParameterKind::String)?);
        }
        parameters.push(parameter("is_valid", is_valid, ParameterKind::Boolean)?);
        parameters.push(parameter("message", message, ParameterKind::String)?);
        let parameters = Parameters::new(parameters).map_err(GraphError::Request)?;
        let shape = StructuredShape::new(SHAPE, None, parameters).map_err(GraphError::Request)?;
        Ok(OutputMode::Structured(shape))
    }

    /// Reads the verdict of the critic's step. A missing structured block, a
    /// block that does not read as the default verdict (without a reader) or
    /// a rejection with a blank message (empty or whitespace only) is
    /// unreadable; an error of a reader is returned as it is.
    pub(crate) fn read(&self, step: &Step) -> Result<Reading, GraphError> {
        let Some(value) = step.structured().next() else {
            return Ok(Reading::Unreadable(NO_BLOCK.to_owned()));
        };
        let verdict = match self {
            VerdictForm::Override(VerdictOverride {
                reader: Some(reader),
                ..
            }) => reader(value)?,
            VerdictForm::Override(_) | VerdictForm::Default { .. } => {
                match serde_json::from_value::<Verdict>(value.clone()) {
                    Ok(verdict) => verdict,
                    Err(error) => return Ok(Reading::Unreadable(error.to_string())),
                }
            }
        };
        if !verdict.is_valid && verdict.message.trim().is_empty() {
            return Ok(Reading::Unreadable(BLANK_REJECTION.to_owned()));
        }
        Ok(Reading::Verdict(verdict))
    }
}

/// Why a step without a structured block is unreadable.
const NO_BLOCK: &str = "the step carries no structured block";

/// Why a rejection with a blank message is unreadable.
const BLANK_REJECTION: &str = "a rejecting verdict carries a blank message";

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
