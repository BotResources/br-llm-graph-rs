//! A runner port kept in memory: the host's port, as a real one would
//! implement it over its jobs and threads. The child's thread is derived from
//! the call's key, and a call asked again under its key gives the end it had
//! instead of running its child again; each runner type answers through a
//! function the host gives.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use br_llm_graph::{
    AnswerBlock, CallKey, RunnerAnswer, RunnerFuture, RunnerPort, RunnerReply, RunnerRequest,
};
use serde_json::{Map, Value as Json};

use super::thread_id_of;

/// What a child does with its arguments: answers with blocks, or fails with
/// a reason.
type Child = Box<dyn Fn(&Map<String, Json>) -> Result<Vec<AnswerBlock>, String> + Send + Sync>;

/// The runners a host reaches, by runner type, and the end of every call it
/// settled, by key.
#[derive(Default)]
pub struct MemoryRunners {
    children: HashMap<String, Child>,
    ended: Mutex<HashMap<CallKey, RunnerReply>>,
}

impl MemoryRunners {
    pub fn new() -> Self {
        Self::default()
    }

    /// The runner type `runner_type`, answering as `child` does.
    pub fn runner(
        mut self,
        runner_type: &str,
        child: impl Fn(&Map<String, Json>) -> Result<Vec<AnswerBlock>, String> + Send + Sync + 'static,
    ) -> Self {
        self.children
            .insert(runner_type.to_owned(), Box::new(child));
        self
    }

    /// The calls whose end was settled.
    pub fn settled(&self) -> usize {
        self.ended().len()
    }

    fn ended(&self) -> MutexGuard<'_, HashMap<CallKey, RunnerReply>> {
        self.ended.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The key as text: what a real port derives its child's thread and job
/// from.
fn key_text(key: &CallKey) -> String {
    let tool_call = key.tool_call.as_ref().map_or("-", |id| id.as_str());
    format!("{}/{}/{tool_call}", key.origin.run, key.origin.occurrence)
}

impl RunnerPort for MemoryRunners {
    fn call<'a>(&'a self, request: &'a RunnerRequest) -> RunnerFuture<'a> {
        Box::pin(async move {
            if let Some(ended) = self.ended().get(&request.key) {
                return Ok(ended.clone());
            }
            let Some(child) = self.children.get(request.target.as_str()) else {
                return Err(format!("no runner of type {}", request.target).into());
            };
            let reply = match child(&request.arguments) {
                Ok(blocks) => RunnerReply::Answered(RunnerAnswer::new(
                    blocks,
                    thread_id_of(&key_text(&request.key)),
                )),
                Err(reason) => RunnerReply::Failed { reason },
            };
            self.ended().insert(request.key.clone(), reply.clone());
            Ok(reply)
        })
    }
}
