use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use br_agentic_contract::{PortError, PortFuture};
use br_agentic_nodes::{
    CriticRequest, CriticRequestPort, ProgrammaticCriticPort, ProgrammaticVerdict,
};
use br_llm_messages::ModelResponse;

use crate::common::{Doc, human_id, text_of};

pub type Log = Arc<Mutex<Vec<String>>>;

pub fn log() -> Log {
    Arc::default()
}

pub fn marks(log: &Log) -> Vec<String> {
    log.lock().unwrap().clone()
}

pub fn mark_on(
    log: &Log,
    name: &'static str,
) -> impl Fn(&br_llm_clients_core::ClientRequest) + Send + Sync + 'static {
    let log = log.clone();
    move |_| log.lock().unwrap().push(name.to_owned())
}

pub enum Verdict {
    Accept,
    Refuse(String),
    Fail,
    Hang,
}

pub fn refuse(reason: &str) -> Verdict {
    Verdict::Refuse(reason.to_owned())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Checked {
    pub text: String,
    pub refusals: u32,
    pub note: String,
}

type Hook = Box<dyn Fn(usize) + Send + Sync>;

pub struct Checker {
    verdicts: Mutex<VecDeque<Verdict>>,
    checked: Mutex<Vec<Checked>>,
    log: Option<Log>,
    hook: Option<Hook>,
}

impl Checker {
    pub fn new(verdicts: Vec<Verdict>) -> Arc<Self> {
        Self::build(verdicts, None, None)
    }

    pub fn logging(verdicts: Vec<Verdict>, log: &Log) -> Arc<Self> {
        Self::build(verdicts, Some(log.clone()), None)
    }

    pub fn hooked(
        verdicts: Vec<Verdict>,
        hook: impl Fn(usize) + Send + Sync + 'static,
    ) -> Arc<Self> {
        Self::build(verdicts, None, Some(Box::new(hook)))
    }

    fn build(verdicts: Vec<Verdict>, log: Option<Log>, hook: Option<Hook>) -> Arc<Self> {
        Arc::new(Self {
            verdicts: Mutex::new(verdicts.into()),
            checked: Mutex::new(Vec::new()),
            log,
            hook,
        })
    }

    pub fn checked(&self) -> Vec<Checked> {
        self.checked.lock().unwrap().clone()
    }

    pub fn refusals_seen(&self) -> Vec<u32> {
        self.checked().iter().map(|call| call.refusals).collect()
    }

    pub fn texts_seen(&self) -> Vec<String> {
        self.checked()
            .iter()
            .map(|call| call.text.clone())
            .collect()
    }
}

impl ProgrammaticCriticPort<Doc> for Checker {
    fn check<'a>(
        &'a self,
        snapshot: &'a Doc,
        response: &'a ModelResponse,
        refusals_in_a_row: u32,
    ) -> PortFuture<'a, Result<ProgrammaticVerdict, PortError>> {
        Box::pin(async move {
            let index = {
                let mut checked = self.checked.lock().unwrap();
                checked.push(Checked {
                    text: text_of(response),
                    refusals: refusals_in_a_row,
                    note: snapshot.note.clone(),
                });
                checked.len()
            };
            if let Some(log) = &self.log {
                log.lock().unwrap().push("programmatic".to_owned());
            }
            if let Some(hook) = &self.hook {
                hook(index);
            }
            for _ in 0..5 {
                tokio::task::yield_now().await;
            }
            let verdict = self.verdicts.lock().unwrap().pop_front();
            match verdict {
                None | Some(Verdict::Accept) => Ok(ProgrammaticVerdict::Accept),
                Some(Verdict::Refuse(reason)) => Ok(ProgrammaticVerdict::Refuse(reason)),
                Some(Verdict::Fail) => Err(PortError::fault("the programmatic critic port failed")),
                Some(Verdict::Hang) => std::future::pending().await,
            }
        })
    }
}

pub enum Requested {
    Text(&'static str),
    Blank,
    Fail,
}

pub struct Requester {
    mode: Requested,
    calls: Mutex<Vec<String>>,
    human: br_llm_messages::HumanId,
}

impl Requester {
    pub fn new(mode: Requested) -> Arc<Self> {
        Arc::new(Self {
            mode,
            calls: Mutex::new(Vec::new()),
            human: human_id(),
        })
    }

    pub fn human(&self) -> br_llm_messages::HumanId {
        self.human
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl CriticRequestPort<Doc> for Requester {
    fn request<'a>(
        &'a self,
        _snapshot: &'a Doc,
        response: &'a ModelResponse,
    ) -> PortFuture<'a, Result<CriticRequest, PortError>> {
        Box::pin(async move {
            let count = {
                let mut calls = self.calls.lock().unwrap();
                calls.push(text_of(response));
                calls.len()
            };
            match self.mode {
                Requested::Fail => Err(PortError::fault("the critic-request port failed")),
                Requested::Blank => Ok(CriticRequest {
                    text: "   ".to_owned(),
                    human: self.human,
                }),
                Requested::Text(prefix) => Ok(CriticRequest {
                    text: format!("{prefix} #{count}"),
                    human: self.human,
                }),
            }
        })
    }
}
