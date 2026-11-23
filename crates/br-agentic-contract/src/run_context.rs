use br_graph_contract::InstanceId;
use br_llm_messages::{AgentId, Author, ConversationId};

use crate::ids::ThreadRef;

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RunContext {
    pub instance: InstanceId,
    pub thread: ThreadRef,
    pub agent: AgentId,
    pub author: Author,
    pub conversation: ConversationId,
}

impl RunContext {
    pub fn new(
        instance: InstanceId,
        thread: ThreadRef,
        agent: AgentId,
        author: Author,
        conversation: ConversationId,
    ) -> Self {
        Self {
            instance,
            thread,
            agent,
            author,
            conversation,
        }
    }
}
