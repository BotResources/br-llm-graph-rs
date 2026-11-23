use br_llm_messages::{ContentBlock, ModelResponse, StructuredBlock};

pub fn structured_reply(response: &ModelResponse) -> Option<&StructuredBlock> {
    response.blocks().iter().find_map(|block| match block {
        ContentBlock::StructuredContent(structured) => Some(structured),
        _ => None,
    })
}
