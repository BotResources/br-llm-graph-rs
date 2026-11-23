use br_agentic_contract::{PortError, PortFuture, ThreadAccess, ThreadRef};
use br_llm_messages::EntryId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredAnswer {
    Stored,
    Dropped,
}

pub trait HarnessHistory: ThreadAccess {
    fn stored<'a>(
        &'a self,
        thread: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<StoredAnswer, PortError>>;
}
