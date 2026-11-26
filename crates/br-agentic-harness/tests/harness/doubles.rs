use std::sync::Arc;

use crate::common::Doc;
use br_agentic_contract::{
    AgentLabel, AiTurnRef, PortError, PortFuture, PromptPort, ResponseStream, ResultStream,
    ResultStreamBox, RunContext, ThreadAccess, ThreadRef, Tool, ToolBlock, ToolCode, ToolContent,
    ToolFault, ToolsPort,
};
use br_agentic_harness::HarnessHistory;
use br_agentic_harness_contract::StoredAnswer;
use br_agentic_harness_contract::memory::MemoryHarnessHistory;
use br_graph_contract::CancelView;
use br_llm_clients_core::{Parameters, ToolSpec};
use br_llm_messages::{
    Conversation, EntryId, InputTurn, ModelResponse, OpaqueJson, RuntimeToolResults, ToolName,
    TurnClosure, TurnEntry,
};
use std::error::Error;

pub struct RefusingStored(pub MemoryHarnessHistory);

impl HarnessHistory for RefusingStored {
    fn stored<'a>(
        &'a self,
        _thread: &'a ThreadRef,
        _entry: EntryId,
    ) -> PortFuture<'a, Result<StoredAnswer, PortError>> {
        Box::pin(async move { Err(PortError::refused("the history refuses to say")) })
    }
}

impl ThreadAccess for RefusingStored {
    fn read<'a>(&'a self, t: &'a ThreadRef) -> PortFuture<'a, Result<Conversation, PortError>> {
        self.0.read(t)
    }

    fn add_input_turn<'a>(
        &'a self,
        t: &'a ThreadRef,
        turn: InputTurn,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.0.add_input_turn(t, turn)
    }

    fn add_entry<'a>(
        &'a self,
        t: &'a ThreadRef,
        turn: AiTurnRef,
        entry: TurnEntry,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.0.add_entry(t, turn, entry)
    }

    fn close_turn<'a>(
        &'a self,
        t: &'a ThreadRef,
        turn: AiTurnRef,
        closure: TurnClosure,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.0.close_turn(t, turn, closure)
    }

    fn open_response<'a>(
        &'a self,
        t: &'a ThreadRef,
        turn: AiTurnRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<ResponseStream, PortError>> {
        self.0.open_response(t, turn, entry)
    }

    fn complete_response<'a>(
        &'a self,
        t: &'a ThreadRef,
        turn: AiTurnRef,
        response: ModelResponse,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.0.complete_response(t, turn, response)
    }

    fn discard_response<'a>(
        &'a self,
        t: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.0.discard_response(t, entry)
    }

    fn open_result<'a>(
        &'a self,
        t: &'a ThreadRef,
        turn: AiTurnRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<ResultStreamBox, PortError>> {
        self.0.open_result(t, turn, entry)
    }

    fn complete_result<'a>(
        &'a self,
        t: &'a ThreadRef,
        turn: AiTurnRef,
        result: RuntimeToolResults,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.0.complete_result(t, turn, result)
    }

    fn discard_result<'a>(
        &'a self,
        t: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.0.discard_result(t, entry)
    }
}

pub struct FixedPrompt(pub String);

impl PromptPort<Doc> for FixedPrompt {
    fn prompt<'a>(
        &'a self,
        _snapshot: &'a Doc,
        _label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<String, PortError>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

pub struct FaultyTools;

impl ToolsPort<Doc> for FaultyTools {
    fn tools<'a>(
        &'a self,
        _snapshot: &'a Doc,
        _context: &'a RunContext,
    ) -> PortFuture<'a, Result<Vec<Tool>, PortError>> {
        Box::pin(async move { Err(PortError::fault("the runner tools are gone")) })
    }
}

pub struct NullSink;

impl ResultStream for NullSink {
    fn send<'a>(&'a mut self, _fragment: String) -> PortFuture<'a, ()> {
        Box::pin(async move {})
    }
}

struct Echo;

impl ToolCode for Echo {
    fn call<'a>(
        &'a self,
        _arguments: &'a OpaqueJson,
        _cancel: CancelView,
        _result: &'a mut dyn ResultStream,
    ) -> PortFuture<'a, Result<ToolContent, ToolFault>> {
        Box::pin(async move {
            let block = ToolBlock::text("from the runner").map_err(ToolFault::new)?;
            Ok(ToolContent::new(vec![block], false))
        })
    }
}

pub fn runner_tool(name: &str) -> Result<Tool, Box<dyn Error>> {
    let spec = ToolSpec::new(ToolName::new(name)?, "a tool", Parameters::new(Vec::new())?)?;
    Ok(Tool::new(spec, Arc::new(Echo)))
}
