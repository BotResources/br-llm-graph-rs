use std::sync::Mutex;

use br_agentic_contract::{PortError, PortFuture};
use br_agentic_nodes::{
    GeneratorCriticOutcome, GeneratorCriticOutcomePort, Judgement, ReactOutcome, ReactOutcomePort,
};
use br_graph_contract::{Fields, Reason};
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
    pub judgement: Option<Judgement>,
    pub steps: Option<u32>,
    pub iterations: Option<u32>,
}

impl Fields for Doc {
    type Update = Recorded;

    fn empty_update() -> Recorded {
        Recorded {
            text: String::new(),
            step_limit: false,
            judgement: None,
            steps: None,
            iterations: None,
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
    react: Mutex<Vec<ReactOutcome>>,
    pair: Mutex<Vec<GeneratorCriticOutcome>>,
    fail: bool,
}

impl Recording {
    pub fn failing() -> Self {
        Self {
            fail: true,
            ..Self::default()
        }
    }

    pub fn react_outcomes(&self) -> Vec<ReactOutcome> {
        self.react.lock().unwrap().clone()
    }

    pub fn pair_outcomes(&self) -> Vec<GeneratorCriticOutcome> {
        self.pair.lock().unwrap().clone()
    }

    pub fn calls(&self) -> usize {
        self.react.lock().unwrap().len() + self.pair.lock().unwrap().len()
    }
}

impl ReactOutcomePort<Doc> for Recording {
    fn update<'a>(
        &'a self,
        _snapshot: &'a Doc,
        outcome: &'a ReactOutcome,
    ) -> PortFuture<'a, Result<Recorded, PortError>> {
        Box::pin(async move {
            if self.fail {
                return Err(PortError::fault("the outcome port failed"));
            }
            self.react.lock().unwrap().push(outcome.clone());
            Ok(Recorded {
                text: text_of(outcome.response()),
                step_limit: outcome.step_limit_reached(),
                judgement: None,
                steps: Some(outcome.steps()),
                iterations: None,
            })
        })
    }
}

impl GeneratorCriticOutcomePort<Doc> for Recording {
    fn update<'a>(
        &'a self,
        _snapshot: &'a Doc,
        outcome: &'a GeneratorCriticOutcome,
    ) -> PortFuture<'a, Result<Recorded, PortError>> {
        Box::pin(async move {
            if self.fail {
                return Err(PortError::fault("the outcome port failed"));
            }
            self.pair.lock().unwrap().push(outcome.clone());
            Ok(Recorded {
                text: text_of(outcome.response()),
                step_limit: outcome.step_limit_reached(),
                judgement: Some(outcome.judgement()),
                steps: None,
                iterations: Some(outcome.iterations()),
            })
        })
    }
}
