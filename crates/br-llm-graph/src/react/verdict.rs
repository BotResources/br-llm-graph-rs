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

/// What a verdict reader gives for the critic's structured block: the
/// verdict, or that the block cannot be read, and why. A failure is the
/// reader's error, which fails the run as returned.
///
/// More readings may come in a later version: a `match` on it outside the
/// crate needs a wildcard arm.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Reading {
    /// The verdict: its `message` is what the generator reads in a
    /// critique, or what an acceptance holds. A rejection whose message is
    /// blank is still an unreadable verdict.
    Verdict(Verdict),
    /// The block cannot be read as a verdict: an unreadable verdict, asked
    /// for again with `reason` while verdict retries are left (the critic
    /// reads `Your reply could not be read as a verdict: {reason}. Reply
    /// again with the verdict.`); after the last retry the run fails with
    /// `Structured`, carrying it.
    Unreadable {
        /// Why the block cannot be read, as the critic will read it.
        reason: String,
    },
}

/// Reads a verdict from the critic's structured block: pure and synchronous.
/// It gives the verdict, its `message` as the generator will read it; or it
/// says that the block cannot be read, with a reason, which makes it an
/// unreadable verdict asked for again with that reason; or it fails, and its
/// error fails the run as returned, without a retry.
///
/// ```
/// use std::sync::Arc;
/// use br_llm_graph::{Reading, Verdict, VerdictReader};
///
/// // A shape with `score` (0 to 10) and `notes`: a score of 7 or more passes.
/// let reader: VerdictReader = Arc::new(|block| {
///     let Some(score) = block["score"].as_i64() else {
///         let reason = "score is not an integer".to_owned();
///         return Ok(Reading::Unreadable { reason });
///     };
///     let notes = block["notes"].as_str().unwrap_or_default().to_owned();
///     Ok(Reading::Verdict(Verdict { is_valid: score >= 7, message: notes }))
/// });
/// let read = reader(&serde_json::json!({ "score": "high" })).unwrap();
/// let reason = "score is not an integer".to_owned();
/// assert_eq!(read, Reading::Unreadable { reason });
/// ```
pub type VerdictReader = Arc<dyn Fn(&Json) -> Result<Reading, GraphError> + Send + Sync>;

/// A verdict shape of the caller's, sent instead of the default one; the
/// descriptions of the critic seat are then ignored.
///
/// Without a `reader`, the block is read as the default verdict, so the build
/// checks the shape: its parameters include `is_valid` (boolean) and
/// `message` (string), both required (`VerdictShapeMismatch`). With a
/// `reader`, the shape is the caller's concern and the reader gives the
/// verdict, or says why the block cannot be read.
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
    /// block that does not read as the default verdict (without a reader), a
    /// block the reader says it cannot read, or a rejection with a blank
    /// message (empty or whitespace only) is unreadable; an error of a reader
    /// is returned as it is.
    pub(crate) fn read(&self, step: &Step) -> Result<Reading, GraphError> {
        let Some(value) = step.structured().next() else {
            return Ok(unreadable(NO_BLOCK));
        };
        let verdict = match self {
            VerdictForm::Override(VerdictOverride {
                reader: Some(reader),
                ..
            }) => match reader(value)? {
                Reading::Verdict(verdict) => verdict,
                unreadable @ Reading::Unreadable { .. } => return Ok(unreadable),
            },
            VerdictForm::Override(_) | VerdictForm::Default { .. } => {
                match serde_json::from_value::<Verdict>(value.clone()) {
                    Ok(verdict) => verdict,
                    Err(error) => return Ok(unreadable(&error.to_string())),
                }
            }
        };
        if !verdict.is_valid && verdict.message.trim().is_empty() {
            return Ok(unreadable(BLANK_REJECTION));
        }
        Ok(Reading::Verdict(verdict))
    }
}

/// An unreadable verdict, for `reason`.
pub(crate) fn unreadable(reason: &str) -> Reading {
    Reading::Unreadable {
        reason: reason.to_owned(),
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
