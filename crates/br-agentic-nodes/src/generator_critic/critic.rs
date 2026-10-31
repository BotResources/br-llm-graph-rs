use std::time::Duration;

use br_agentic_contract::AiTurnRef;
use br_llm_clients_core::{BoxFuture, ClientRequest, OutputMode, StructuredShape};
use br_llm_messages::{
    AgentId, Author, ClosureReason, ContentBlock, Conversation, HumanMessage, IncomingEntry,
    InputTurn, ModelResponse, Perspective, PlainBlock, PlainText, ProjectedThread, ResponseEnding,
    ResponseMetadata, TextBlock, Turn,
};

use super::answer::{Answer, critic_shape, unwrap_answer};
use crate::client::{FallbackList, ModelClient};
use crate::fault::{AttemptError, CallFailure, Fault};
use crate::ids::{block_id, entry_id, turn_id};
use crate::ports::CriticRequest;
use crate::seats::Seat;
use crate::sink::DiscardSink;
use crate::slot::{Attempt, Slot, drive};
use crate::thinking::check_thinking;
use crate::worker::Worker;

fn visible_to_critic(entry: &IncomingEntry) -> bool {
    !matches!(
        entry,
        IncomingEntry::AgentMail(_) | IncomingEntry::RuntimeNotice(_)
    )
}

fn critic_view(agent: AgentId, conversation: &Conversation) -> Result<ProjectedThread, Fault> {
    let projected = Perspective::of(agent)
        .keeping(visible_to_critic)
        .project(conversation);
    let (id, agent) = (projected.conversation(), projected.agent());
    let mut turns = Vec::new();
    for turn in projected.into_turns() {
        match turn {
            Turn::InputTurn(input) => {
                let entries: Vec<IncomingEntry> = input
                    .entries()
                    .iter()
                    .filter(|entry| visible_to_critic(entry))
                    .cloned()
                    .collect();
                if !entries.is_empty() {
                    let kept =
                        InputTurn::new_projected(input.id(), entries).map_err(Fault::value)?;
                    turns.push(Turn::InputTurn(kept));
                }
            }
            other => turns.push(other),
        }
    }
    ProjectedThread::from_turns(id, agent, turns).map_err(Fault::value)
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
    system: PlainText,
    history: ProjectedThread,
    request: &'s CriticRequest,
    timeout: Duration,
}

fn shape_of(client: &ModelClient) -> Result<StructuredShape, Fault> {
    critic_shape(!client.should_have_thought())
}

impl Attempt for CriticAttempt<'_> {
    type Output = Answer;

    fn attempt<'b>(
        &'b mut self,
        client: &'b ModelClient,
    ) -> BoxFuture<'b, Result<Answer, AttemptError>> {
        Box::pin(async move {
            let history = with_request(self.history.clone(), self.request)?;
            let request = ClientRequest::new(
                Some(self.system.clone()),
                history,
                turn_id()?,
                entry_id()?,
                Vec::new(),
                OutputMode::Structured(shape_of(client)?),
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
    worker: &Worker<'_>,
    seat: &Seat,
    request: &CriticRequest,
    list: &FallbackList,
) -> Result<Answer, Fault> {
    let system = seat
        .system
        .clone()
        .ok_or_else(|| Fault::Value("the critic has no system prompt".to_owned()))?;
    let history = critic_view(seat.agent, worker.conversation())?;
    let mut attempt = CriticAttempt {
        system,
        history,
        request,
        timeout: worker.timeouts.model_call(),
    };
    let mut slot = Slot::new(list.clone());
    drive(&mut slot, &mut attempt).await
}

pub(crate) async fn store_message(
    worker: &mut Worker<'_>,
    agent: AgentId,
    author: &Author,
    message: &str,
) -> Result<(), Fault> {
    let turn = AiTurnRef {
        id: turn_id()?,
        agent,
        author: author.clone(),
    };
    let text = TextBlock::new(block_id()?, message).map_err(Fault::value)?;
    let response = ModelResponse::new(
        entry_id()?,
        vec![ContentBlock::Text(text)],
        ResponseEnding::finished(),
        ResponseMetadata::new(),
    )
    .map_err(Fault::value)?;
    worker.session.complete_response(&turn, response).await?;
    worker.close_turn(&turn, ClosureReason::completed()).await
}
