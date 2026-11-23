use br_agentic_contract::structured_reply;
use br_llm_messages::ModelResponse;

use crate::fault::CallFailure;

pub fn check_structure(response: &ModelResponse) -> Result<(), CallFailure> {
    if structured_reply(response).is_none() {
        return Err(CallFailure::NoStructuredReply);
    }
    Ok(())
}
