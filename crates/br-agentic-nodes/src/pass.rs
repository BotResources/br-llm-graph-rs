use br_llm_messages::{ClosureReason, ModelResponse, TurnEntry, TurnId};

use crate::fault::{Fault, Stop};
use crate::slot::Slot;
use crate::step::{at_boundary, is_final};
use crate::warning::warning_text;
use crate::worker::Worker;

#[derive(Debug, Clone)]
pub(crate) enum PassEnd {
    Final(ModelResponse),
    StepLimit(ModelResponse),
}

impl PassEnd {
    pub(crate) fn into_response(self) -> ModelResponse {
        match self {
            PassEnd::Final(response) | PassEnd::StepLimit(response) => response,
        }
    }

    pub(crate) fn step_limit_reached(&self) -> bool {
        matches!(self, PassEnd::StepLimit(_))
    }
}

impl Worker<'_> {
    pub(crate) fn restored(&self) -> Option<PassEnd> {
        let last = self.session.conversation().turns().last()?.as_ai()?;
        if last.agent() != self.context.agent {
            return None;
        }
        let response = last.entries().iter().rev().find_map(|entry| match entry {
            TurnEntry::ModelResponse(response) => Some(response),
            _ => None,
        })?;
        let state = last.state();
        if matches!(state.closure(), Some(ClosureReason::BudgetExhausted { .. })) {
            return Some(PassEnd::StepLimit(response.clone()));
        }
        is_final(response, &state).then(|| PassEnd::Final(response.clone()))
    }

    pub(crate) async fn pass(&mut self, slot: &mut Slot) -> Result<PassEnd, Stop> {
        let mut turn: Option<TurnId> = None;
        let mut warn = false;
        loop {
            let warning = if warn {
                warning_text(self.steps.remaining())
            } else {
                None
            };
            let response = self.step(slot, &mut turn, warning.as_deref()).await?;
            let open = turn.ok_or_else(|| Fault::Value("a step left no AI turn".to_owned()))?;
            let target = self.turn_ref(open)?;
            let state = self.turn_state(open)?;
            if is_final(&response, &state) {
                self.close_turn(&target, ClosureReason::completed()).await?;
                return Ok(PassEnd::Final(response));
            }
            if self.steps.remaining() == 0 {
                let reason = ClosureReason::budget_exhausted()
                    .with_detail("the step limit was reached")
                    .map_err(Fault::value)?;
                self.close_turn(&target, reason).await?;
                return Ok(PassEnd::StepLimit(response));
            }
            if response.runtime_calls().next().is_some() {
                self.tool_round(open, &response).await?;
                if at_boundary(&self.turn_state(open)?) {
                    self.drain().await?;
                }
                warn = true;
            } else {
                warn = false;
            }
        }
    }
}
