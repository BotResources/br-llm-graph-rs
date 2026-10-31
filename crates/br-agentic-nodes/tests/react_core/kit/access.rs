use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_agentic_contract::memory::MemoryThreadAccess;
use br_agentic_contract::{
    AiTurnRef, PortError, PortFuture, ResponseStream, ResultStreamBox, ThreadAccess, ThreadRef,
};
use br_llm_clients_core::{BoxFuture, StreamSink};
use br_llm_messages::{
    Conversation, EntryId, InputTurn, ModelResponse, ResponseDelta, RuntimeToolResults,
    TurnClosure, TurnEntry,
};

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Fragment {
    #[default]
    Take,
    Delay(Duration),
    Hold,
}

#[derive(Clone, Default)]
pub struct Quirks {
    pub open_response_delay: Duration,
    pub open_result_yields: bool,
    pub fragment: Fragment,
    pub fail_open_response: bool,
    pub fail_complete_response: bool,
    pub fail_complete_result: bool,
    pub complete_result_takes: Duration,
}

pub struct Wrapped {
    inner: Arc<MemoryThreadAccess>,
    quirks: Quirks,
    attempts: Mutex<HashMap<&'static str, usize>>,
    inflight: AtomicUsize,
    peak: AtomicUsize,
}

impl Wrapped {
    pub fn new(inner: &Arc<MemoryThreadAccess>, quirks: Quirks) -> Arc<Self> {
        Arc::new(Self {
            inner: inner.clone(),
            quirks,
            attempts: Mutex::new(HashMap::new()),
            inflight: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
        })
    }

    pub fn attempts(&self, name: &str) -> usize {
        self.attempts
            .lock()
            .unwrap()
            .get(name)
            .copied()
            .unwrap_or(0)
    }

    pub fn peak_completions(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }

    fn bump(&self, name: &'static str) {
        *self.attempts.lock().unwrap().entry(name).or_default() += 1;
    }
}

struct Held {
    inner: ResponseStream,
    fragment: Fragment,
}

impl StreamSink for Held {
    fn send<'a>(&'a mut self, delta: ResponseDelta) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            match self.fragment {
                Fragment::Take => {}
                Fragment::Delay(delay) => tokio::time::sleep(delay).await,
                Fragment::Hold => std::future::pending::<()>().await,
            }
            self.inner.send(delta).await;
        })
    }
}

fn refused() -> PortError {
    PortError::fault("the test access refuses")
}

impl ThreadAccess for Wrapped {
    fn read<'a>(
        &'a self,
        thread: &'a ThreadRef,
    ) -> PortFuture<'a, Result<Conversation, PortError>> {
        self.inner.read(thread)
    }

    fn add_input_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: InputTurn,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.add_input_turn(thread, turn)
    }

    fn add_entry<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: TurnEntry,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.add_entry(thread, turn, entry)
    }

    fn close_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        closure: TurnClosure,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.bump("close_turn");
        self.inner.close_turn(thread, turn, closure)
    }

    fn open_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<ResponseStream, PortError>> {
        self.bump("open_response");
        Box::pin(async move {
            if self.quirks.fail_open_response {
                return Err(refused());
            }
            if !self.quirks.open_response_delay.is_zero() {
                tokio::time::sleep(self.quirks.open_response_delay).await;
            }
            let inner = self.inner.open_response(thread, turn, entry).await?;
            let held: ResponseStream = Box::new(Held {
                inner,
                fragment: self.quirks.fragment,
            });
            Ok(held)
        })
    }

    fn complete_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        response: ModelResponse,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.bump("complete_response");
        if self.quirks.fail_complete_response {
            return Box::pin(async { Err(refused()) });
        }
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
        self.bump("open_result");
        Box::pin(async move {
            if self.quirks.open_result_yields {
                tokio::task::yield_now().await;
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
        self.bump("complete_result");
        if self.quirks.fail_complete_result {
            return Box::pin(async { Err(refused()) });
        }
        Box::pin(async move {
            let now = self.inflight.fetch_add(1, Ordering::SeqCst) + 1;
            self.peak.fetch_max(now, Ordering::SeqCst);
            if !self.quirks.complete_result_takes.is_zero() {
                tokio::time::sleep(self.quirks.complete_result_takes).await;
            }
            let stored = self.inner.complete_result(thread, turn, result).await;
            self.inflight.fetch_sub(1, Ordering::SeqCst);
            stored
        })
    }

    fn discard_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.discard_result(thread, entry)
    }
}
