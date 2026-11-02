use br_llm_messages::{ContentBlock, ModelResponse, StructuredBlock};

use crate::fault::CallFailure;

pub fn structured_reply(response: &ModelResponse) -> Option<&StructuredBlock> {
    response.blocks().iter().find_map(|block| match block {
        ContentBlock::StructuredContent(structured) => Some(structured),
        _ => None,
    })
}

pub fn check_structure(response: &ModelResponse) -> Result<(), CallFailure> {
    if structured_reply(response).is_none() {
        return Err(CallFailure::NoStructuredReply);
    }
    Ok(())
}
