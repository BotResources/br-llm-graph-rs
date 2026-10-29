use std::sync::Mutex;

use br_agentic_contract::PortFault;
use br_agentic_nodes::{Outcome, OutcomePort};
use br_graph_contract::{Fields, Reason};
use br_llm_clients_core::BoxFuture;
use br_llm_messages::{ContentBlock, ModelResponse};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Doc {
    pub note: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Recorded {
    pub text: String,
    pub step_limit: bool,
    pub verdict: Option<bool>,
}

impl Fields for Doc {
    type Update = Recorded;

    fn empty_update() -> Recorded {
        Recorded {
            text: String::new(),
            step_limit: false,
            verdict: None,
        }
    }

    fn validate(&self) -> Result<(), Reason> {
        Ok(())
    }

    fn apply(&mut self, update: Recorded) -> Result<(), Reason> {
        self.note = update.text;
        Ok(())
    }
}

pub fn text_of(response: &ModelResponse) -> String {
    response
        .blocks()
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.text().as_str().to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

#[derive(Default)]
pub struct Recording {
    outcomes: Mutex<Vec<Outcome>>,
    fail: bool,
}

impl Recording {
    pub fn failing() -> Self {
        Self {
            outcomes: Mutex::new(Vec::new()),
            fail: true,
        }
    }

    pub fn outcomes(&self) -> Vec<Outcome> {
        self.outcomes.lock().unwrap().clone()
    }
}

impl OutcomePort<Doc> for Recording {
    fn update<'a>(
        &'a self,
        _snapshot: &'a Doc,
        outcome: &'a Outcome,
    ) -> BoxFuture<'a, Result<Recorded, PortFault>> {
        Box::pin(async move {
            if self.fail {
                return Err(PortFault::new("the outcome port failed"));
            }
            self.outcomes.lock().unwrap().push(outcome.clone());
            Ok(Recorded {
                text: text_of(outcome.response()),
                step_limit: outcome.step_limit_reached(),
                verdict: outcome.verdict(),
            })
        })
    }
}
