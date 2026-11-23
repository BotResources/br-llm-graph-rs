use std::sync::atomic::{AtomicUsize, Ordering};

use br_agentic_contract::{
    AiTurnRef, PortError, PortFuture, ResponseStream, ResultStream, ResultStreamBox, StreamSink,
    ThreadAccess, ThreadRef,
};
use br_llm_messages::{
    Conversation, ConversationId, EntryId, InputTurn, ModelResponse, ResponseDelta,
    RuntimeToolResults, TurnClosure, TurnEntry,
};

pub struct ForgetfulThreadAccess {
    conversation: ConversationId,
    reads: AtomicUsize,
}

impl ForgetfulThreadAccess {
    pub fn new(conversation: ConversationId) -> Self {
        Self {
            conversation,
            reads: AtomicUsize::new(0),
        }
    }

    pub fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }
}

struct Discarded;

impl StreamSink for Discarded {
    fn send<'a>(&'a mut self, _delta: ResponseDelta) -> PortFuture<'a, ()> {
        Box::pin(async {})
    }
}

impl ResultStream for Discarded {
    fn send<'a>(&'a mut self, _fragment: String) -> PortFuture<'a, ()> {
        Box::pin(async {})
    }
}

fn accepted<'a>() -> PortFuture<'a, Result<(), PortError>> {
    Box::pin(async { Ok(()) })
}

impl ThreadAccess for ForgetfulThreadAccess {
    fn read<'a>(
        &'a self,
        _thread: &'a ThreadRef,
    ) -> PortFuture<'a, Result<Conversation, PortError>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move { Ok(Conversation::new(self.conversation)) })
    }

    fn add_input_turn<'a>(
        &'a self,
        _thread: &'a ThreadRef,
        _turn: InputTurn,
    ) -> PortFuture<'a, Result<(), PortError>> {
        accepted()
    }

    fn add_entry<'a>(
        &'a self,
        _thread: &'a ThreadRef,
        _turn: AiTurnRef,
        _entry: TurnEntry,
    ) -> PortFuture<'a, Result<(), PortError>> {
        accepted()
    }

    fn close_turn<'a>(
        &'a self,
        _thread: &'a ThreadRef,
        _turn: AiTurnRef,
        _closure: TurnClosure,
    ) -> PortFuture<'a, Result<(), PortError>> {
        accepted()
    }

    fn open_response<'a>(
        &'a self,
        _thread: &'a ThreadRef,
        _turn: AiTurnRef,
        _entry: EntryId,
    ) -> PortFuture<'a, Result<ResponseStream, PortError>> {
        Box::pin(async { Ok(Box::new(Discarded) as ResponseStream) })
    }

    fn complete_response<'a>(
        &'a self,
        _thread: &'a ThreadRef,
        _turn: AiTurnRef,
        _response: ModelResponse,
    ) -> PortFuture<'a, Result<(), PortError>> {
        accepted()
    }

    fn discard_response<'a>(
        &'a self,
        _thread: &'a ThreadRef,
        _entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        accepted()
    }

    fn open_result<'a>(
        &'a self,
        _thread: &'a ThreadRef,
        _turn: AiTurnRef,
        _entry: EntryId,
    ) -> PortFuture<'a, Result<ResultStreamBox, PortError>> {
        Box::pin(async { Ok(Box::new(Discarded) as ResultStreamBox) })
    }

    fn complete_result<'a>(
        &'a self,
        _thread: &'a ThreadRef,
        _turn: AiTurnRef,
        _result: RuntimeToolResults,
    ) -> PortFuture<'a, Result<(), PortError>> {
        accepted()
    }

    fn discard_result<'a>(
        &'a self,
        _thread: &'a ThreadRef,
        _entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        accepted()
    }
}

#[tokio::test]
async fn given_a_thread_access_that_keeps_nothing_then_every_call_is_accepted_and_the_read_gives_an_empty_conversation()
 {
    use br_llm_messages::{ClosureReason, TurnClosure};

    use crate::common::*;

    let identity = conversation_id();
    let access = ForgetfulThreadAccess::new(identity);
    let thread = thread_ref();
    let turn = AiTurnRef {
        id: turn_id(),
        agent: agent_id(),
        author: author("agent"),
    };

    access
        .add_entry(
            &thread,
            turn.clone(),
            TurnEntry::from(response_text(entry_id(), "one")),
        )
        .await
        .unwrap();
    access
        .close_turn(
            &thread,
            turn.clone(),
            TurnClosure::new(entry_id(), ClosureReason::completed()),
        )
        .await
        .unwrap();
    let mut stream = access.open_result(&thread, turn, entry_id()).await.unwrap();
    stream.send("ignored".to_owned()).await;
    access.discard_result(&thread, entry_id()).await.unwrap();
    let read = access.read(&thread).await.unwrap();

    assert!(read.turns().is_empty());
    assert_eq!(read.id(), identity);
    assert_eq!(access.reads(), 1);
}
