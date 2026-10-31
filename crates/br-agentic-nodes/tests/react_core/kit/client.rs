use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_agentic_nodes::{FallbackList, ModelClient, ThinkingExpectation};
use br_llm_clients_core::{
    BoxFuture, CacheRequirement, CallOutcome, CallReport, Client, ClientError, ClientRequest,
    ClientSettings, MaxOutputTokens, ReasoningLevel, StreamSink,
};
use br_llm_messages::{
    ContentBlock, DeltaOp, ExecutionStatus, ModelName, ModelResponse, OpaqueJson,
    ProviderToolExecutionRecord, RedactedThinkingBlock, ReplayFormat, ReplayMaterial,
    ReplayPayload, ResponseDelta, ResponseEnding, ResponseMetadata, RuntimeToolCall, TextBlock,
    ThinkingBlock, ToolName,
};

use crate::common::block_id;

#[derive(Clone)]
pub enum Part {
    Text(String),
    Call(String, String),
    Thinking,
    Redacted,
    EmptyThinking,
    Pending,
}

pub fn blocks(parts: &[Part]) -> Vec<ContentBlock> {
    parts
        .iter()
        .map(|part| match part {
            Part::Text(text) => ContentBlock::Text(TextBlock::new(block_id(), text).unwrap()),
            Part::Call(name, arguments) => ContentBlock::RuntimeToolCall(
                RuntimeToolCall::new(
                    block_id(),
                    ToolName::new(name).unwrap(),
                    OpaqueJson::parse(arguments).unwrap(),
                )
                .unwrap(),
            ),
            Part::Thinking => ContentBlock::Thinking(ThinkingBlock::new(block_id(), "hm").unwrap()),
            Part::Redacted => {
                ContentBlock::RedactedThinking(RedactedThinkingBlock::new(block_id(), material()))
            }
            Part::EmptyThinking => {
                ContentBlock::Thinking(ThinkingBlock::replay_only(block_id(), material()))
            }
            Part::Pending => ContentBlock::ProviderToolExecutionRecord(
                ProviderToolExecutionRecord::new(block_id()).with_status(ExecutionStatus::Pending),
            ),
        })
        .collect()
}

fn material() -> ReplayMaterial {
    ReplayMaterial::new(
        ReplayFormat::new("test").unwrap(),
        ReplayPayload::text("sealed"),
    )
}

enum Kind {
    Respond(Vec<Part>, ResponseEnding),
    Fail(ClientError),
    Hang,
}

pub struct Act {
    kind: Kind,
    delay: Duration,
    delta: bool,
}

impl Act {
    fn of(kind: Kind) -> Self {
        Self {
            kind,
            delay: Duration::ZERO,
            delta: false,
        }
    }

    pub fn text(text: &str) -> Self {
        Self::of(Kind::Respond(
            vec![Part::Text(text.to_owned())],
            ResponseEnding::finished(),
        ))
    }

    pub fn call(name: &str) -> Self {
        Self::call_with(name, "{}")
    }

    pub fn call_with(name: &str, arguments: &str) -> Self {
        Self::calls_with(vec![(name, arguments)])
    }

    pub fn calls(names: &[&str]) -> Self {
        Self::calls_with(names.iter().map(|name| (*name, "{}")).collect())
    }

    fn calls_with(calls: Vec<(&str, &str)>) -> Self {
        let parts = calls
            .into_iter()
            .map(|(name, arguments)| Part::Call(name.to_owned(), arguments.to_owned()))
            .collect();
        Self::of(Kind::Respond(parts, ResponseEnding::tool_use()))
    }

    pub fn shaped(parts: Vec<Part>, ending: ResponseEnding) -> Self {
        Self::of(Kind::Respond(parts, ending))
    }

    pub fn ended(text: &str, ending: ResponseEnding) -> Self {
        Self::shaped(vec![Part::Text(text.to_owned())], ending)
    }

    pub fn continuation() -> Self {
        Self::ended("continuing", ResponseEnding::provider_continuation())
    }

    pub fn pending_execution() -> Self {
        Self::shaped(
            vec![Part::Text("searching".to_owned()), Part::Pending],
            ResponseEnding::finished(),
        )
    }

    pub fn fail(error: ClientError) -> Self {
        Self::of(Kind::Fail(error))
    }

    pub fn hang() -> Self {
        Self::of(Kind::Hang)
    }

    pub fn thinking(mut self) -> Self {
        if let Kind::Respond(parts, _) = &mut self.kind {
            parts.insert(0, Part::Thinking);
        }
        self
    }

    pub fn after(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn streaming(mut self) -> Self {
        self.delta = true;
        self
    }
}

type Hook = Box<dyn Fn(&ClientRequest) + Send + Sync>;

pub struct Model {
    acts: Mutex<VecDeque<Act>>,
    requests: Mutex<Vec<ClientRequest>>,
    settings: ClientSettings,
    settings_reads: AtomicUsize,
    hook: Mutex<Option<Hook>>,
}

impl Model {
    pub fn new(acts: Vec<Act>) -> Arc<Self> {
        Arc::new(Self {
            acts: Mutex::new(acts.into()),
            requests: Mutex::new(Vec::new()),
            settings: ClientSettings::new(
                ModelName::new("model").unwrap(),
                ReasoningLevel::Off,
                CacheRequirement::NotRequired,
                MaxOutputTokens::new(1024).unwrap(),
            ),
            settings_reads: AtomicUsize::new(0),
            hook: Mutex::new(None),
        })
    }

    pub fn on_call(&self, hook: impl Fn(&ClientRequest) + Send + Sync + 'static) {
        *self.hook.lock().unwrap() = Some(Box::new(hook));
    }

    pub fn requests(&self) -> Vec<ClientRequest> {
        self.requests.lock().unwrap().clone()
    }

    pub fn calls(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    pub fn settings_reads(&self) -> usize {
        self.settings_reads.load(Ordering::SeqCst)
    }
}

fn response(request: &ClientRequest, parts: &[Part], ending: &ResponseEnding) -> ModelResponse {
    ModelResponse::new(
        request.reply_entry(),
        blocks(parts),
        ending.clone(),
        ResponseMetadata::new(),
    )
    .unwrap()
}

impl Client for Model {
    fn call<'a>(
        &'a self,
        request: &'a ClientRequest,
        sink: &'a mut dyn StreamSink,
    ) -> BoxFuture<'a, CallOutcome> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request.clone());
            if let Some(hook) = self.hook.lock().unwrap().as_ref() {
                hook(request);
            }
            let Some(act) = self.acts.lock().unwrap().pop_front() else {
                return CallOutcome::failure(
                    CallReport::new(Duration::ZERO),
                    ClientError::Transport,
                );
            };
            if !act.delay.is_zero() {
                tokio::time::sleep(act.delay).await;
            }
            if act.delta {
                let started = DeltaOp::response_started(ResponseMetadata::new());
                sink.send(ResponseDelta::new(
                    request.reply_turn(),
                    request.reply_entry(),
                    started,
                ))
                .await;
            }
            match act.kind {
                Kind::Hang => std::future::pending::<CallOutcome>().await,
                Kind::Fail(error) => CallOutcome::failure(CallReport::new(Duration::ZERO), error),
                Kind::Respond(parts, ending) => CallOutcome::success(
                    CallReport::new(Duration::ZERO),
                    response(request, &parts, &ending),
                )
                .unwrap(),
            }
        })
    }

    fn settings(&self) -> &ClientSettings {
        self.settings_reads.fetch_add(1, Ordering::SeqCst);
        &self.settings
    }
}

pub fn plain(model: &Arc<Model>) -> ModelClient {
    ModelClient::new(model.clone(), ThinkingExpectation::NotExpected)
}

pub fn expecting(model: &Arc<Model>) -> ModelClient {
    ModelClient::new(model.clone(), ThinkingExpectation::Expected)
}

pub fn chain(clients: Vec<ModelClient>) -> FallbackList {
    FallbackList::new(clients).unwrap()
}

pub fn plains(models: &[&Arc<Model>]) -> FallbackList {
    chain(models.iter().map(|model| plain(model)).collect())
}
