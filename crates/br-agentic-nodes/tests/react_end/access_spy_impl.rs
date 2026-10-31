use br_agentic_contract::{
    AiTurnRef, PortError, PortFuture, ResponseStream, ResultStreamBox, ThreadAccess, ThreadRef,
};
use br_llm_messages::{
    Conversation, EntryId, InputTurn, ModelResponse, RuntimeToolResults, TurnClosure, TurnEntry,
};

use crate::access_spy::{AccessSpy, NullResult, NullSink, Op};

impl ThreadAccess for AccessSpy {
    fn read<'a>(
        &'a self,
        thread: &'a ThreadRef,
    ) -> PortFuture<'a, Result<Conversation, PortError>> {
        Box::pin(async move {
            self.gate(Op::Read, None, None).await?;
            match &self.kept {
                Some(kept) => Ok(kept.clone()),
                None => self.inner.read(thread).await,
            }
        })
    }

    fn add_input_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: InputTurn,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async move {
            self.gate(Op::AddInputTurn, None, None).await?;
            if self.forgets() {
                return Ok(());
            }
            self.inner.add_input_turn(thread, turn).await
        })
    }

    fn add_entry<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: TurnEntry,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async move {
            self.gate(Op::AddEntry, Some(turn.agent), None).await?;
            if self.forgets() {
                return Ok(());
            }
            self.inner.add_entry(thread, turn, entry).await
        })
    }

    fn close_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        closure: TurnClosure,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async move {
            self.gate(
                Op::CloseTurn,
                Some(turn.agent),
                Some(closure.reason().clone()),
            )
            .await?;
            if self.forgets() {
                return Ok(());
            }
            self.inner.close_turn(thread, turn, closure).await
        })
    }

    fn open_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<ResponseStream, PortError>> {
        Box::pin(async move {
            self.gate(Op::OpenResponse, Some(turn.agent), None).await?;
            if self.forgets() {
                let stream: ResponseStream = Box::new(NullSink);
                return Ok(stream);
            }
            self.inner.open_response(thread, turn, entry).await
        })
    }

    fn complete_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        response: ModelResponse,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async move {
            self.gate(Op::CompleteResponse, Some(turn.agent), None)
                .await?;
            if self.forgets() {
                return Ok(());
            }
            self.inner.complete_response(thread, turn, response).await
        })
    }

    fn discard_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async move {
            self.gate(Op::DiscardResponse, None, None).await?;
            if self.forgets() {
                return Ok(());
            }
            self.inner.discard_response(thread, entry).await
        })
    }

    fn open_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<ResultStreamBox, PortError>> {
        Box::pin(async move {
            self.gate(Op::OpenResult, Some(turn.agent), None).await?;
            if self.forgets() {
                let stream: ResultStreamBox = Box::new(NullResult);
                return Ok(stream);
            }
            self.inner.open_result(thread, turn, entry).await
        })
    }

    fn complete_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        result: RuntimeToolResults,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async move {
            self.gate(Op::CompleteResult, Some(turn.agent), None)
                .await?;
            if self.forgets() {
                return Ok(());
            }
            self.inner.complete_result(thread, turn, result).await
        })
    }

    fn discard_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async move {
            self.gate(Op::DiscardResult, None, None).await?;
            if self.forgets() {
                return Ok(());
            }
            self.inner.discard_result(thread, entry).await
        })
    }
}
