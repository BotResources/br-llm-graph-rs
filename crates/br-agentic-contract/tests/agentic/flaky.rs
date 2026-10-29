use br_agentic_contract::memory::MemoryThreadAccess;
use br_agentic_contract::{Incoming, ThreadAccess, ThreadRef, TurnTarget};
use br_llm_messages::{
    AgentId, BlockId, EntryId, ModelResponse, ProjectedThread, RuntimeToolResult, TurnId,
};

pub struct Flaky {
    pub inner: MemoryThreadAccess,
    pub fail_adds: bool,
    pub fail_discards: bool,
}

fn fault(text: &str) -> br_agentic_contract::PortFault {
    br_agentic_contract::PortFault::new(text.to_owned())
}

impl ThreadAccess for Flaky {
    fn load<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
    ) -> br_llm_clients_core::BoxFuture<'a, Result<ProjectedThread, br_agentic_contract::PortFault>>
    {
        self.inner.load(thread, agent)
    }

    fn add_input_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        turn: TurnId,
        items: Vec<Incoming>,
    ) -> br_llm_clients_core::BoxFuture<
        'a,
        Result<br_agentic_contract::Stored, br_agentic_contract::ThreadError>,
    > {
        if self.fail_adds {
            return Box::pin(async { Err(fault("add refused").into()) });
        }
        self.inner.add_input_turn(thread, agent, turn, items)
    }

    fn add_incoming<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        target: TurnTarget,
        item: Incoming,
    ) -> br_llm_clients_core::BoxFuture<
        'a,
        Result<br_agentic_contract::Stored, br_agentic_contract::ThreadError>,
    > {
        if self.fail_adds {
            return Box::pin(async { Err(fault("add refused").into()) });
        }
        self.inner.add_incoming(thread, agent, target, item)
    }

    fn close_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        turn: TurnId,
        closure: br_llm_messages::TurnClosure,
    ) -> br_llm_clients_core::BoxFuture<
        'a,
        Result<br_agentic_contract::Stored, br_agentic_contract::ThreadError>,
    > {
        self.inner.close_turn(thread, agent, turn, closure)
    }

    fn open_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        target: TurnTarget,
        entry: EntryId,
    ) -> br_llm_clients_core::BoxFuture<
        'a,
        Result<
            br_agentic_contract::Opened<br_agentic_contract::ResponseStream, ModelResponse>,
            br_agentic_contract::ThreadError,
        >,
    > {
        self.inner.open_response(thread, agent, target, entry)
    }

    fn complete_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        target: TurnTarget,
        response: ModelResponse,
    ) -> br_llm_clients_core::BoxFuture<
        'a,
        Result<br_agentic_contract::Completed<ModelResponse>, br_agentic_contract::ThreadError>,
    > {
        self.inner
            .complete_response(thread, agent, target, response)
    }

    fn discard_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        entry: EntryId,
    ) -> br_llm_clients_core::BoxFuture<'a, Result<(), br_agentic_contract::ThreadError>> {
        if self.fail_discards {
            return Box::pin(async { Err(fault("discard failed").into()) });
        }
        self.inner.discard_response(thread, agent, entry)
    }

    fn open_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        target: TurnTarget,
        entry: EntryId,
        call: BlockId,
    ) -> br_llm_clients_core::BoxFuture<
        'a,
        Result<
            br_agentic_contract::Opened<
                br_agentic_contract::ResultStreamBox,
                br_llm_messages::RuntimeToolResults,
            >,
            br_agentic_contract::ThreadError,
        >,
    > {
        self.inner.open_result(thread, agent, target, entry, call)
    }

    fn complete_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        target: TurnTarget,
        entry: EntryId,
        result: RuntimeToolResult,
    ) -> br_llm_clients_core::BoxFuture<
        'a,
        Result<
            br_agentic_contract::Completed<br_llm_messages::RuntimeToolResults>,
            br_agentic_contract::ThreadError,
        >,
    > {
        self.inner
            .complete_result(thread, agent, target, entry, result)
    }

    fn discard_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        entry: EntryId,
    ) -> br_llm_clients_core::BoxFuture<'a, Result<(), br_agentic_contract::ThreadError>> {
        if self.fail_discards {
            return Box::pin(async { Err(fault("discard failed").into()) });
        }
        self.inner.discard_result(thread, agent, entry)
    }
}
