use br_agentic_contract::{Completed, Opened, TurnTarget};
use br_llm_clients_core::{BoxFuture, ClientRequest, OutputMode};
use br_llm_messages::{
    ModelResponse, ProjectedThread, ResponseEnding, RuntimeNotice, TurnId, TurnState,
};

use crate::client::ModelClient;
use crate::fault::{AttemptError, CallFailure, Fault};
use crate::ids::{entry_id, turn_id};
use crate::slot::{Attempt, Slot, drive};
use crate::thinking::check_thinking;
use crate::worker::Worker;

enum Obtained {
    Fresh(ModelResponse),
    Held(ModelResponse),
}

struct StepAttempt<'w, 'a> {
    worker: &'w mut Worker<'a>,
    history: ProjectedThread,
    target: TurnTarget,
    turn: TurnId,
}

impl Attempt for StepAttempt<'_, '_> {
    type Output = Obtained;

    fn attempt<'b>(
        &'b mut self,
        client: &'b ModelClient,
    ) -> BoxFuture<'b, Result<Obtained, AttemptError>> {
        Box::pin(async move {
            let entry = entry_id()?;
            let request = ClientRequest::new(
                self.worker.system.clone(),
                self.history.clone(),
                self.turn,
                entry,
                self.worker.tools.specs(),
                OutputMode::Plain,
            )
            .map_err(|error| AttemptError::Fatal(Fault::value(error)))?;
            let opened = self
                .worker
                .session
                .open_response(self.target.clone(), entry)
                .await?;
            let mut stream = match opened {
                Opened::Held(held) => return Ok(Obtained::Held(held)),
                Opened::Stream(stream) => stream,
            };
            let timeout = self.worker.timeouts.model_call();
            let called =
                tokio::time::timeout(timeout, client.client().call(&request, &mut *stream)).await;
            drop(stream);
            let checked = match called {
                Err(_) => Err(CallFailure::Timeout(timeout)),
                Ok(outcome) => outcome
                    .into_parts()
                    .1
                    .map_err(CallFailure::from)
                    .and_then(|response| check_thinking(client, &response).map(|()| response)),
            };
            match checked {
                Ok(response) => Ok(Obtained::Fresh(response)),
                Err(failure) => {
                    self.worker.session.discard_response(entry).await?;
                    Err(AttemptError::Failed(failure))
                }
            }
        })
    }
}

pub(crate) fn is_final(state: &TurnState) -> bool {
    state.outstanding_runtime_calls().is_empty()
        && state.pending_provider_executions().is_empty()
        && state.last_ending() != Some(&ResponseEnding::provider_continuation())
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
        let mut history = self.load().await?;
        if let (Some(text), Some(open)) = (warning, *turn) {
            let notice = RuntimeNotice::new(entry_id()?, text).map_err(Fault::value)?;
            history
                .push_incoming_into(&open, notice)
                .map_err(Fault::value)?;
        }
        let target = TurnTarget::Ai {
            id: turn_id,
            author: self.author.clone(),
        };
        let obtained = {
            let mut attempt = StepAttempt {
                worker: self,
                history,
                target: target.clone(),
                turn: turn_id,
            };
            drive(slot, &mut attempt).await?
        };
        let response = match obtained {
            Obtained::Held(held) => held,
            Obtained::Fresh(fresh) => {
                match self
                    .session
                    .complete_response(target, fresh.clone())
                    .await?
                {
                    Completed::Added => fresh,
                    Completed::Held(held) => held,
                }
            }
        };
        *turn = Some(turn_id);
        self.steps.count();
        Ok(response)
    }

    pub(crate) async fn turn_state(&self, turn: TurnId) -> Result<TurnState, Fault> {
        let projected = self.load().await?;
        let found = projected.turn(&turn).map_err(Fault::value)?;
        found
            .as_ai()
            .map(br_llm_messages::AITurn::state)
            .ok_or_else(|| Fault::Value("the turn of the pass is not an AI turn".to_owned()))
    }
}
