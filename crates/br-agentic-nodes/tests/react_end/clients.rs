use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use br_llm_clients_core::{
    BoxFuture, CacheRequirement, CallOutcome, CallReport, Client, ClientRequest, ClientSettings,
    MaxOutputTokens, ReasoningLevel, StreamSink,
};
use br_llm_messages::{
    ContentBlock, EntryId, ModelName, ModelResponse, ResponseEnding, ResponseMetadata, TextBlock,
};

use crate::common::block_id;

pub struct ReusedId {
    id: EntryId,
    calls: AtomicUsize,
    settings: ClientSettings,
}

impl ReusedId {
    pub fn of(id: EntryId) -> Arc<Self> {
        Arc::new(Self {
            id,
            calls: AtomicUsize::new(0),
            settings: ClientSettings::new(
                ModelName::new("reused").unwrap(),
                ReasoningLevel::Off,
                CacheRequirement::NotRequired,
                MaxOutputTokens::new(1024).unwrap(),
            ),
        })
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl Client for ReusedId {
    fn call<'a>(
        &'a self,
        _request: &'a ClientRequest,
        _sink: &'a mut dyn StreamSink,
    ) -> BoxFuture<'a, CallOutcome> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            let response = ModelResponse::new(
                self.id,
                vec![ContentBlock::Text(
                    TextBlock::new(block_id(), "a reused identity").unwrap(),
                )],
                ResponseEnding::finished(),
                ResponseMetadata::new(),
            )
            .unwrap();
            CallOutcome::success(CallReport::new(Duration::ZERO), response).unwrap()
        })
    }

    fn settings(&self) -> &ClientSettings {
        &self.settings
    }
}
