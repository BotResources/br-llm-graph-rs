use br_agentic_contract::{Incoming, ThreadAccess, ThreadRef, WrittenEntry};
use br_llm_clients_core::{BoxFuture, ClientRequest, OutputMode, StructuredShape};
use br_llm_messages::{
    AgentId, Author, HumanMessage, IncomingEntry, InputTurn, PlainBlock, PlainText,
    ProjectedThread, TextBlock, Turn,
};
use std::time::Duration;

use super::answer::{Answer, unwrap_answer};
use crate::client::ModelClient;
use crate::fault::{AttemptError, CallFailure, Fault};
use crate::ids::{block_id, entry_id, turn_id};
use crate::ports::CriticRequest;
use crate::sink::DiscardSink;
use crate::slot::{Attempt, Slot, drive};
use crate::thinking::check_thinking;

pub(crate) struct Seat {
    pub(crate) agent: AgentId,
    pub(crate) author: Author,
    pub(crate) system: PlainText,
    pub(crate) request: CriticRequest,
}

fn shows_agent_output(turn: &InputTurn) -> bool {
    turn.entries().iter().any(|entry| {
        matches!(
            entry,
            IncomingEntry::AgentOutput(_) | IncomingEntry::PartialAgentOutput(_)
        )
    })
}

fn with_request(
    mut history: ProjectedThread,
    request: &CriticRequest,
) -> Result<ProjectedThread, Fault> {
    let text = TextBlock::new(block_id()?, request.text.as_str()).map_err(Fault::value)?;
    let message = HumanMessage::new(entry_id()?, request.human, vec![PlainBlock::Text(text)])
        .map_err(Fault::value)?;
    let last_input = match history.turns().last() {
        Some(Turn::InputTurn(input)) if !shows_agent_output(input) => Some(input.id()),
        _ => None,
    };
    match last_input {
        Some(turn) => history
            .push_incoming_into(&turn, message)
            .map_err(Fault::value)?,
        None => {
            let turn = InputTurn::new(turn_id()?, vec![IncomingEntry::from(message)])
                .map_err(Fault::value)?;
            history
                .push_turn(Turn::InputTurn(turn))
                .map_err(Fault::value)?;
        }
    }
    Ok(history)
}

struct CriticAttempt<'s> {
    seat: &'s Seat,
    history: ProjectedThread,
    shape: &'s StructuredShape,
    timeout: Duration,
}

impl Attempt for CriticAttempt<'_> {
    type Output = Answer;

    fn attempt<'b>(
        &'b mut self,
        client: &'b ModelClient,
    ) -> BoxFuture<'b, Result<Answer, AttemptError>> {
        Box::pin(async move {
            let history = with_request(self.history.clone(), &self.seat.request)?;
            let request = ClientRequest::new(
                Some(self.seat.system.clone()),
                history,
                turn_id()?,
                entry_id()?,
                Vec::new(),
                OutputMode::Structured(self.shape.clone()),
            )
            .map_err(|error| AttemptError::Fatal(Fault::value(error)))?;
            let mut sink = DiscardSink;
            let called =
                tokio::time::timeout(self.timeout, client.client().call(&request, &mut sink)).await;
            let checked = match called {
                Err(_) => Err(CallFailure::Timeout(self.timeout)),
                Ok(outcome) => {
                    outcome
                        .into_parts()
                        .1
                        .map_err(CallFailure::from)
                        .and_then(|response| {
                            check_thinking(client, &response)?;
                            unwrap_answer(&response).map_err(CallFailure::UnusableAnswer)
                        })
                }
            };
            checked.map_err(AttemptError::Failed)
        })
    }
}

pub(crate) async fn call_critic(
    access: &dyn ThreadAccess,
    thread: &ThreadRef,
    seat: &Seat,
    slot: &mut Slot,
    shape: &StructuredShape,
    timeout: Duration,
) -> Result<Answer, Fault> {
    let history = access.load(thread, seat.agent).await?;
    let mut attempt = CriticAttempt {
        seat,
        history,
        shape,
        timeout,
    };
    drive(slot, &mut attempt).await
}

pub(crate) async fn store_message(
    access: &dyn ThreadAccess,
    thread: &ThreadRef,
    seat: &Seat,
    message: String,
) -> Result<(), Fault> {
    let text = TextBlock::new(block_id()?, message).map_err(Fault::value)?;
    let written = WrittenEntry {
        id: entry_id()?,
        sender: seat.agent,
        author: seat.author.clone(),
        content: vec![PlainBlock::Text(text)],
    };
    access
        .add_input_turn(
            thread,
            seat.agent,
            turn_id()?,
            vec![Incoming::Written(written)],
        )
        .await?;
    Ok(())
}
