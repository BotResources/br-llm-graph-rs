use std::time::Duration;

use br_agentic_contract::{AiTurnRef, Session};
use br_llm_clients_core::{BoxFuture, ClientRequest, OutputMode, ToolSpec};
use br_llm_messages::{
    AITurn, ModelResponse, PlainText, ProjectedThread, ResponseEnding, RuntimeNotice, TurnId,
    TurnState,
};

use crate::client::ModelClient;
use crate::fault::{AttemptError, CallFailure, Fault};
use crate::ids::{entry_id, turn_id};
use crate::slot::{Attempt, Slot, drive};
use crate::thinking::check_thinking;
use crate::worker::Worker;

struct StepAttempt<'w, 'a> {
    session: &'w mut Session<'a>,
    system: Option<PlainText>,
    history: ProjectedThread,
    turn: AiTurnRef,
    tools: Vec<ToolSpec>,
    timeout: Duration,
    covered: bool,
}

impl Attempt for StepAttempt<'_, '_> {
    type Output = ModelResponse;

    fn attempt<'b>(
        &'b mut self,
        client: &'b ModelClient,
    ) -> BoxFuture<'b, Result<ModelResponse, AttemptError>> {
        Box::pin(async move {
            let entry = entry_id()?;
            let request = ClientRequest::new(
                self.system.clone(),
                self.history.clone(),
                self.turn.id,
                entry,
                self.tools.clone(),
                OutputMode::Plain,
            )
            .map_err(|error| AttemptError::Fatal(Fault::value(error)))?;
            let mut stream = self.session.open_response(&self.turn, entry).await?;
            let called =
                tokio::time::timeout(self.timeout, client.client().call(&request, &mut *stream))
                    .await;
            drop(stream);
            let checked = match called {
                Err(_) => Err(CallFailure::Timeout(self.timeout)),
                Ok(outcome) => {
                    outcome
                        .into_parts()
                        .1
                        .map_err(CallFailure::from)
                        .and_then(|response| {
                            if self.covered && !exempt(&response) {
                                check_thinking(client, &response)?;
                            }
                            Ok(response)
                        })
                }
            };
            match checked {
                Ok(response) => Ok(response),
                Err(failure) => {
                    self.session.discard_response(entry).await?;
                    Err(AttemptError::Failed(failure))
                }
            }
        })
    }
}

fn exempt(response: &ModelResponse) -> bool {
    response.runtime_calls().next().is_none()
        && matches!(
            response.ending(),
            ResponseEnding::Refusal { .. }
                | ResponseEnding::Filtering { .. }
                | ResponseEnding::OutputExhaustion { .. }
        )
}

pub(crate) fn at_boundary(state: &TurnState) -> bool {
    state.outstanding_runtime_calls().is_empty()
        && state.pending_provider_executions().is_empty()
        && state.last_ending() != Some(&ResponseEnding::provider_continuation())
}

pub(crate) fn is_final(response: &ModelResponse, state: &TurnState) -> bool {
    response.runtime_calls().next().is_none() && at_boundary(state)
}

impl Worker<'_> {
    pub async fn step(
        &mut self,
        slot: &mut Slot,
        turn: &mut Option<TurnId>,
        warning: Option<&str>,
    ) -> Result<ModelResponse, Fault> {
        let turn_id = match *turn {
            Some(turn) => turn,
            None => turn_id()?,
        };
        let mut history = self.session.conversation().project_for(self.context.agent);
        if let (Some(text), Some(open)) = (warning, *turn) {
            let notice = RuntimeNotice::new(entry_id()?, text).map_err(Fault::value)?;
            history
                .push_incoming_into(&open, notice)
                .map_err(Fault::value)?;
        }
        let target = self.turn_ref(turn_id)?;
        let response = {
            let mut attempt = StepAttempt {
                session: &mut self.session,
                system: self.system.clone(),
                history,
                turn: target.clone(),
                tools: self.tools.specs(),
                timeout: self.timeouts.model_call(),
                covered: !slot.is_settled(),
            };
            drive(slot, &mut attempt).await?
        };
        self.session
            .complete_response(&target, response.clone())
            .await?;
        *turn = Some(turn_id);
        self.steps.count();
        Ok(response)
    }

    pub(crate) fn ai_turn(&self, turn: TurnId) -> Result<&AITurn, Fault> {
        self.session
            .conversation()
            .turn(&turn)
            .map_err(Fault::value)?
            .as_ai()
            .ok_or_else(|| Fault::Value("the turn of the pass is not an AI turn".to_owned()))
    }

    pub(crate) fn turn_state(&self, turn: TurnId) -> Result<TurnState, Fault> {
        Ok(self.ai_turn(turn)?.state())
    }
}
