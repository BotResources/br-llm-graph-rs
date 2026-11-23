use std::fmt::{Display, Write};

use br_agentic_contract::{PortError, ToolBlock, ToolContent, ToolFault};

pub(crate) fn value(error: impl Display) -> PortError {
    PortError::fault(error.to_string())
}

pub(crate) fn error(message: impl Into<String>) -> Result<ToolContent, ToolFault> {
    ToolContent::error_text(message).map_err(ToolFault::new)
}

pub(crate) fn acknowledgement(message: impl Into<String>) -> Result<ToolContent, ToolFault> {
    let block = ToolBlock::text(message).map_err(ToolFault::new)?;
    Ok(ToolContent::new(vec![block], false))
}

pub(crate) fn invalid(reason: impl Display) -> Result<ToolContent, ToolFault> {
    error(format!("the arguments are invalid: {reason}"))
}

pub(crate) fn quote(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for character in text.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            control if control.is_control() => {
                let _ = write!(quoted, "\\u{{{:x}}}", u32::from(control));
            }
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}
