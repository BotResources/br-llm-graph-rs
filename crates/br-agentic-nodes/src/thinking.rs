use br_llm_messages::{ContentBlock, ModelResponse};

use crate::client::ModelClient;
use crate::fault::CallFailure;

pub fn holds_thinking(response: &ModelResponse) -> bool {
    response.blocks().iter().any(|block| {
        matches!(
            block,
            ContentBlock::Thinking(_) | ContentBlock::RedactedThinking(_)
        )
    })
}

pub fn check_thinking(client: &ModelClient, response: &ModelResponse) -> Result<(), CallFailure> {
    if client.should_have_thought() && !holds_thinking(response) {
        return Err(CallFailure::NoThinking);
    }
    Ok(())
}
