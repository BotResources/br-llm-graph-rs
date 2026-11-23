use br_agentic_contract::PortError;
use br_llm_clients_core::{Parameter, ParameterKind, Parameters, ToolSpec};
use br_llm_messages::ToolName;

use crate::tool_text::value;

pub const SEND_MAIL: &str = "send_mail";
pub const GET_MAIL_FILES: &str = "get_mail_files";

fn parameter(
    name: &str,
    description: &str,
    required: bool,
    kind: ParameterKind,
) -> Result<Parameter, PortError> {
    Parameter::new(name, description, required, kind).map_err(value)
}

fn parameters(list: Vec<Parameter>) -> Result<Parameters, PortError> {
    Parameters::new(list).map_err(value)
}

fn agents() -> ParameterKind {
    ParameterKind::Array {
        items: Box::new(ParameterKind::String),
    }
}

pub(crate) fn send_mail_spec() -> Result<ToolSpec, PortError> {
    let attachment = parameters(vec![
        parameter(
            "name",
            "The name of the attachment.",
            true,
            ParameterKind::String,
        )?,
        parameter(
            "document",
            "The reference of the document to attach.",
            true,
            ParameterKind::String,
        )?,
    ])?;
    let list = parameters(vec![
        parameter(
            "to",
            "The identities of the agents that the mail is for. At least one.",
            true,
            agents(),
        )?,
        parameter(
            "cc",
            "The identities of the agents that get the mail in copy.",
            false,
            agents(),
        )?,
        parameter(
            "subject",
            "The subject of the mail.",
            true,
            ParameterKind::String,
        )?,
        parameter("body", "The body of the mail.", true, ParameterKind::String)?,
        parameter(
            "attachments",
            "The documents to attach to the mail.",
            false,
            ParameterKind::Array {
                items: Box::new(ParameterKind::Object {
                    parameters: attachment,
                }),
            },
        )?,
    ])?;
    ToolSpec::new(
        ToolName::new(SEND_MAIL).map_err(value)?,
        "Send a mail to other agents. The answer comes later, as a mail.",
        list,
    )
    .map_err(value)
}

pub(crate) fn get_mail_files_spec() -> Result<ToolSpec, PortError> {
    let list = parameters(vec![parameter(
        "mail_id",
        "The identity of a mail that you received.",
        true,
        ParameterKind::String,
    )?])?;
    ToolSpec::new(
        ToolName::new(GET_MAIL_FILES).map_err(value)?,
        "Get the documents that are attached to a mail you received.",
        list,
    )
    .map_err(value)
}
