use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_llm_clients_core::{
    BoxFuture, CacheRequirement, CallOutcome, CallReport, Client, ClientRequest, ClientSettings,
    MaxOutputTokens, ReasoningLevel, StreamSink,
};
use br_llm_messages::{
    ContentBlock, DeltaOp, ModelName, ModelResponse, OpaqueJson, ResponseDelta, ResponseEnding,
    ResponseMetadata, StructuredBlock,
};

use crate::common::block_id;

pub struct RawReply {
    structured: Option<String>,
    ending: ResponseEnding,
}

impl RawReply {
    pub fn structured(json: &str, ending: ResponseEnding) -> Self {
        Self {
            structured: Some(json.to_owned()),
            ending,
        }
    }

    pub fn answer(validated: bool, message: &str, ending: ResponseEnding) -> Self {
        let json = format!("{{\"is_validated\":{validated},\"message\":{message:?}}}");
        Self::structured(&json, ending)
    }
}

pub struct RawClient {
    replies: Mutex<VecDeque<RawReply>>,
    requests: Mutex<Vec<ClientRequest>>,
    settings: ClientSettings,
    streams: bool,
}

impl RawClient {
    pub fn new(replies: Vec<RawReply>) -> Arc<Self> {
        Self::build(replies, false)
    }

    pub fn streaming(replies: Vec<RawReply>) -> Arc<Self> {
        Self::build(replies, true)
    }

    fn build(replies: Vec<RawReply>, streams: bool) -> Arc<Self> {
        Arc::new(Self {
            streams,
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
            settings: ClientSettings::new(
                ModelName::new("raw").unwrap(),
                ReasoningLevel::Off,
                CacheRequirement::NotRequired,
                MaxOutputTokens::new(1024).unwrap(),
            ),
        })
    }

    pub fn calls(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

fn build(request: &ClientRequest, reply: RawReply) -> ModelResponse {
    let mut blocks = Vec::new();
    if let Some(json) = reply.structured {
        blocks.push(ContentBlock::StructuredContent(
            StructuredBlock::new(block_id(), OpaqueJson::parse(&json).unwrap()).unwrap(),
        ));
    }
    ModelResponse::new(
        request.reply_entry(),
        blocks,
        reply.ending,
        ResponseMetadata::new(),
    )
    .unwrap()
}

impl Client for RawClient {
    fn call<'a>(
        &'a self,
        request: &'a ClientRequest,
        sink: &'a mut dyn StreamSink,
    ) -> BoxFuture<'a, CallOutcome> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request.clone());
            if self.streams {
                let delta = ResponseDelta::new(
                    request.reply_turn(),
                    request.reply_entry(),
                    DeltaOp::text_appended(block_id(), "streamed"),
                );
                sink.send(delta).await;
            }
            let reply = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("the raw script has a reply left");
            CallOutcome::success(CallReport::new(Duration::ZERO), build(request, reply)).unwrap()
        })
    }

    fn settings(&self) -> &ClientSettings {
        &self.settings
    }
}
