//! A runner port kept in memory: each target answers as the test scripts
//! it, every call is kept with its key, a held target or call waits until
//! it is released, and a call dropped before it ends is counted. Its
//! `RunnerPort` implementation is in `runners_port`.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::task::{Poll, Waker};

use br_llm_messages::Text;

use crate::runner::{AnswerBlock, CallKey, RunnerAnswer, RunnerError, RunnerReply, RunnerRequest};
use crate::value::ThreadId;

/// The error of the test port: the text a test broke a target with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortError(pub String);

impl std::fmt::Display for PortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PortError {}

/// How a target answers a call: its blocks, a failure, or a broken port.
type Script = Box<dyn Fn(&RunnerRequest) -> Scripted + Send + Sync>;

/// What a script gives for one call.
pub enum Scripted {
    /// The child answers these blocks.
    Answer(Vec<AnswerBlock>),
    /// The child fails with this reason.
    Fail(String),
    /// The port fails with this text.
    Break(String),
}

#[derive(Default)]
pub(crate) struct Inner {
    scripts: HashMap<String, Script>,
    /// The calls received, in arrival order.
    pub(crate) calls: Vec<RunnerRequest>,
    /// The keys of the calls dropped before they ended.
    pub(crate) dropped: Vec<CallKey>,
    /// The keys seen, in order: the n-th answers in the n-th thread.
    threads: Vec<CallKey>,
    held: HashSet<String>,
    held_calls: HashSet<CallKey>,
    /// The calls waiting at a hold, and their wakers.
    pub(crate) waiting: usize,
    pub(crate) held_wakers: Vec<Waker>,
    /// The waits for waiting calls (`waiting`), woken when a call waits.
    pub(crate) watchers: Vec<Waker>,
    pub(crate) live: usize,
    pub(crate) peak: usize,
}

impl Inner {
    /// Whether `request` waits: its target or its key is held.
    pub(crate) fn holds(&self, request: &RunnerRequest) -> bool {
        self.held.contains(request.target.as_str()) || self.held_calls.contains(&request.key)
    }
}

/// The runner port of the tests.
#[derive(Default)]
pub struct TestRunners {
    inner: Mutex<Inner>,
}

/// The text block `text`.
pub fn text_block(text: &str) -> AnswerBlock {
    AnswerBlock::Text(Text::new(text).unwrap())
}

impl TestRunners {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `target` answers each call as `script` says.
    pub fn script(
        &self,
        target: &str,
        script: impl Fn(&RunnerRequest) -> Scripted + Send + Sync + 'static,
    ) {
        self.lock()
            .scripts
            .insert(target.to_owned(), Box::new(script));
    }

    /// `target` answers each call with `blocks`.
    pub fn answer(&self, target: &str, blocks: Vec<AnswerBlock>) {
        self.script(target, move |_| Scripted::Answer(blocks.clone()));
    }

    /// `target` fails each call with `reason`.
    pub fn fail(&self, target: &str, reason: &str) {
        let reason = reason.to_owned();
        self.script(target, move |_| Scripted::Fail(reason.clone()));
    }

    /// The port fails each call of `target` with `PortError(text)`.
    pub fn break_down(&self, target: &str, text: &str) {
        let text = text.to_owned();
        self.script(target, move |_| Scripted::Break(text.clone()));
    }

    /// The calls of `target` wait until it is released.
    pub fn hold(&self, target: &str) {
        self.lock().held.insert(target.to_owned());
    }

    /// The call `key` waits until it is released.
    pub fn hold_call(&self, key: CallKey) {
        self.lock().held_calls.insert(key);
    }

    /// Lets the calls of `target` go on.
    pub fn release(&self, target: &str) {
        self.let_go(|inner| {
            inner.held.remove(target);
        });
    }

    /// Lets the call `key` go on.
    pub fn release_call(&self, key: &CallKey) {
        self.let_go(|inner| {
            inner.held_calls.remove(key);
        });
    }

    fn let_go(&self, change: impl FnOnce(&mut Inner)) {
        let wakers = {
            let mut inner = self.lock();
            change(&mut inner);
            std::mem::take(&mut inner.held_wakers)
        };
        wakers.into_iter().for_each(Waker::wake);
    }

    /// Resolves once `count` calls wait at a hold.
    pub fn waiting(&self, count: usize) -> impl Future<Output = ()> + '_ {
        std::future::poll_fn(move |cx| {
            let mut inner = self.lock();
            if inner.waiting >= count {
                return Poll::Ready(());
            }
            inner.watchers.push(cx.waker().clone());
            Poll::Pending
        })
    }

    /// The calls received, in arrival order.
    pub fn calls(&self) -> Vec<RunnerRequest> {
        self.lock().calls.clone()
    }

    /// The keys of the calls received, in arrival order.
    pub fn keys(&self) -> Vec<CallKey> {
        self.calls().into_iter().map(|call| call.key).collect()
    }

    /// The keys of the calls dropped before they ended.
    pub fn dropped(&self) -> Vec<CallKey> {
        self.lock().dropped.clone()
    }

    /// The most calls under way at once.
    pub fn peak(&self) -> usize {
        self.lock().peak
    }

    /// The thread the child of the call `key` answers in: the same for the
    /// same key.
    pub fn thread_of(&self, key: &CallKey) -> ThreadId {
        let mut inner = self.lock();
        let number = match inner.threads.iter().position(|seen| seen == key) {
            Some(index) => index + 1,
            None => {
                inner.threads.push(key.clone());
                inner.threads.len()
            }
        };
        ThreadId::new(format!("01900000-0000-7000-9000-{number:012x}")).unwrap()
    }

    /// The reply of `request`, as its target's script says.
    pub(crate) fn reply(&self, request: &RunnerRequest) -> Result<RunnerReply, RunnerError> {
        let scripted = {
            let inner = self.lock();
            let Some(script) = inner.scripts.get(request.target.as_str()) else {
                return Err(PortError(format!("no script for runner {}", request.target)).into());
            };
            script(request)
        };
        match scripted {
            Scripted::Answer(blocks) => Ok(RunnerReply::Answered(RunnerAnswer::new(
                blocks,
                self.thread_of(&request.key),
            ))),
            Scripted::Fail(reason) => Ok(RunnerReply::Failed { reason }),
            Scripted::Break(text) => Err(PortError(text).into()),
        }
    }
}
