use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_llm_clients_core::{
    BoxFuture, CacheRequirement, CallOutcome, CallReport, Client, ClientError, ClientRequest,
    ClientSettings, MaxOutputTokens, ReasoningLevel, StreamSink,
};
use br_llm_messages::{
    ContentBlock, ModelName, ModelResponse, OpaqueJson, ResponseEnding, ResponseMetadata,
    RuntimeToolCall, StructuredBlock, TextBlock, ThinkingBlock, ToolName,
};

use super::ids::block_id;

pub enum Reply {
    Text(String),
    Calls(Vec<(String, String)>),
    Structured(String),
    Continuation,
    CallsThenContinuation(String),
    Fail(ClientError),
    Hang,
    Thinking(Box<Reply>),
}

impl Reply {
    pub fn text(text: &str) -> Self {
        Reply::Text(text.to_owned())
    }

    pub fn call(name: &str) -> Self {
        Reply::Calls(vec![(name.to_owned(), "{}".to_owned())])
    }

    pub fn call_then_continuation(name: &str) -> Self {
        Reply::CallsThenContinuation(name.to_owned())
    }

    pub fn calls(names: &[&str]) -> Self {
        Reply::Calls(
            names
                .iter()
                .map(|name| ((*name).to_owned(), "{}".to_owned()))
                .collect(),
        )
    }

    pub fn call_with(name: &str, arguments: &str) -> Self {
        Reply::Calls(vec![(name.to_owned(), arguments.to_owned())])
    }

    pub fn answer(validated: bool, message: &str) -> Self {
        Reply::Structured(format!(
            "{{\"is_validated\":{validated},\"message\":{message:?}}}"
        ))
    }

    pub fn thinking(reply: Reply) -> Self {
        Reply::Thinking(Box::new(reply))
    }
}

type Hook = Box<dyn Fn(&ClientRequest) + Send + Sync>;

pub struct Scripted {
    replies: Mutex<VecDeque<Reply>>,
    requests: Mutex<Vec<ClientRequest>>,
    settings: ClientSettings,
    hook: Mutex<Option<Hook>>,
}

impl Scripted {
    pub fn new(replies: Vec<Reply>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
            settings: ClientSettings::new(
                ModelName::new("scripted").unwrap(),
                ReasoningLevel::Off,
                CacheRequirement::NotRequired,
                MaxOutputTokens::new(1024).unwrap(),
            ),
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
}

fn response(request: &ClientRequest, reply: &Reply) -> ModelResponse {
    let (blocks, ending) = match reply {
        Reply::Text(text) => (
            vec![ContentBlock::Text(
                TextBlock::new(block_id(), text).unwrap(),
            )],
            ResponseEnding::finished(),
        ),
        Reply::Calls(calls) => (
            calls
                .iter()
                .map(|(name, arguments)| {
                    ContentBlock::RuntimeToolCall(
                        RuntimeToolCall::new(
                            block_id(),
                            ToolName::new(name).unwrap(),
                            OpaqueJson::parse(arguments).unwrap(),
                        )
                        .unwrap(),
                    )
                })
                .collect(),
            ResponseEnding::tool_use(),
        ),
        Reply::Structured(json) => (
            vec![ContentBlock::StructuredContent(
                StructuredBlock::new(block_id(), OpaqueJson::parse(json).unwrap()).unwrap(),
            )],
            ResponseEnding::finished(),
        ),
        Reply::Continuation => (
            vec![ContentBlock::Text(
                TextBlock::new(block_id(), "continuing").unwrap(),
            )],
            ResponseEnding::provider_continuation(),
        ),
        Reply::CallsThenContinuation(name) => {
            let (blocks, _) = parts(request, &Reply::call(name));
            (blocks, ResponseEnding::provider_continuation())
        }
        Reply::Thinking(inner) => {
            let (mut blocks, ending) = parts(request, inner);
            blocks.insert(
                0,
                ContentBlock::Thinking(ThinkingBlock::new(block_id(), "hm").unwrap()),
            );
            (blocks, ending)
        }
        Reply::Fail(_) | Reply::Hang => (Vec::new(), ResponseEnding::finished()),
    };
    ModelResponse::new(
        request.reply_entry(),
        blocks,
        ending,
        ResponseMetadata::new(),
    )
    .unwrap()
}

fn parts(request: &ClientRequest, reply: &Reply) -> (Vec<ContentBlock>, ResponseEnding) {
    let built = response(request, reply);
    (built.blocks().to_vec(), built.ending().clone())
}

async fn serve(request: &ClientRequest, reply: Reply) -> CallOutcome {
    match reply {
        Reply::Hang => std::future::pending::<CallOutcome>().await,
        Reply::Fail(error) => CallOutcome::failure(CallReport::new(Duration::ZERO), error),
        other => CallOutcome::success(CallReport::new(Duration::ZERO), response(request, &other))
            .unwrap(),
    }
}

impl Client for Scripted {
    fn call<'a>(
        &'a self,
        request: &'a ClientRequest,
        _sink: &'a mut dyn StreamSink,
    ) -> BoxFuture<'a, CallOutcome> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request.clone());
            if let Some(hook) = self.hook.lock().unwrap().as_ref() {
                hook(request);
            }
            let reply = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("the script has a reply left");
            serve(request, reply).await
        })
    }

    fn settings(&self) -> &ClientSettings {
        &self.settings
    }
}
