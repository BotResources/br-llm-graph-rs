//! A runner port kept in memory: the host's port, as a real one would
//! implement it over its jobs and threads. The child's thread is derived from
//! the call's key, and a call asked again under its key gives the end it had
//! instead of running its child again; a call dropped before it settled its
//! child cancels that child, once (`ChildGuard`); each runner type answers
//! through a function the host gives.

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
    cancelled: Mutex<Vec<CallKey>>,
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

    /// The calls whose child was cancelled because the call was dropped.
    pub fn cancelled_children(&self) -> Vec<CallKey> {
        self.cancelled().clone()
    }

    fn ended(&self) -> MutexGuard<'_, HashMap<CallKey, RunnerReply>> {
        self.ended.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn cancelled(&self) -> MutexGuard<'_, Vec<CallKey>> {
        self.cancelled
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// The key as text: what a real port derives its child's thread and job
/// from.
fn key_text(key: &CallKey) -> String {
    let tool_call = key.tool_call.as_ref().map_or("-", |id| id.as_str());
    format!(
        "{}/{}/{}/{tool_call}",
        key.origin.run, key.origin.occurrence, key.arguments
    )
}

/// The guard of a child under way: armed when the child is created, it
/// cancels the child when the call is dropped before it decided the child's
/// fate (a cancel of the run), once; disarmed when the call settles it.
struct ChildGuard<'a> {
    runners: &'a MemoryRunners,
    key: &'a CallKey,
    armed: bool,
}

impl ChildGuard<'_> {
    /// The call decided the child's fate: the guard steps aside.
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for ChildGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            // A real port sends its jobs service the cancel of the child's
            // job here; this one keeps it.
            self.runners.cancelled().push(self.key.clone());
        }
    }
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
            // The child is created: from here, a drop cancels it.
            let guard = ChildGuard {
                runners: self,
                key: &request.key,
                armed: true,
            };
            // The wait for the child's answer, where a cancel of the run
            // drops the call.
            tokio::task::yield_now().await;
            let reply = match child(&request.arguments) {
                Ok(blocks) => RunnerReply::Answered(RunnerAnswer::new(
                    blocks,
                    thread_id_of(&key_text(&request.key)),
                )),
                Err(reason) => RunnerReply::Failed { reason },
            };
            self.ended().insert(request.key.clone(), reply.clone());
            guard.disarm();
            Ok(reply)
        })
    }
}
