use std::sync::Arc;

use br_agentic_contract::{
    AiTurnRef, PortError, PortFuture, ResponseStream, ResultStreamBox, ThreadAccess, ThreadRef,
};
use br_agentic_harness_contract::HarnessHistory;
use br_llm_messages::{
    Conversation, EntryId, InputTurn, ModelResponse, RuntimeToolResults, TurnClosure, TurnEntry,
};

use crate::mail_set::{conversation_mails, input_turn_mails, turn_entry_mail};
use crate::shared::Shared;

pub struct HistoryAdapter {
    inner: Arc<dyn HarnessHistory>,
    shared: Arc<Shared>,
}

impl HistoryAdapter {
    pub(crate) fn new(inner: Arc<dyn HarnessHistory>, shared: Arc<Shared>) -> Self {
        Self { inner, shared }
    }
}

impl ThreadAccess for HistoryAdapter {
    fn read<'a>(
        &'a self,
        thread: &'a ThreadRef,
    ) -> PortFuture<'a, Result<Conversation, PortError>> {
        Box::pin(async move {
            let conversation = self.inner.read(thread).await?;
            self.shared
                .mails
                .record(*thread, conversation_mails(&conversation));
            Ok(conversation)
        })
    }

    fn add_input_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: InputTurn,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async move {
            let mails = input_turn_mails(&turn);
            self.inner.add_input_turn(thread, turn).await?;
            self.shared.mails.record(*thread, mails);
            Ok(())
        })
    }

    fn add_entry<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: TurnEntry,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async move {
            let mail = turn_entry_mail(&entry);
            self.inner.add_entry(thread, turn, entry).await?;
            self.shared.mails.record(*thread, mail);
            Ok(())
        })
    }

    fn close_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        closure: TurnClosure,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.close_turn(thread, turn, closure)
    }

    fn open_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<ResponseStream, PortError>> {
        self.inner.open_response(thread, turn, entry)
    }

    fn complete_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        response: ModelResponse,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.complete_response(thread, turn, response)
    }

    fn discard_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.discard_response(thread, entry)
    }

    fn open_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<ResultStreamBox, PortError>> {
        self.inner.open_result(thread, turn, entry)
    }

    fn complete_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        result: RuntimeToolResults,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.complete_result(thread, turn, result)
    }

    fn discard_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.discard_result(thread, entry)
    }
}
