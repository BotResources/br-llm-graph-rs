use br_llm_messages::{
    AITurn, Conversation, Error, InputTurn, ModelResponse, RuntimeToolResults, Turn, TurnClosure,
    TurnEntry, TurnId,
};

use crate::error::PortError;
use crate::ids::MailId;
use crate::thread_access::AiTurnRef;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SessionError {
    #[error("the rules of br-llm-messages refuse the change: {0}")]
    Rules(#[source] Error),
    #[error("the turn {0} is not the AI turn of that agent and author")]
    WrongTurn(TurnId),
    #[error("the entry built for the mail {0} is not the entry of that mail")]
    WrongMailEntry(MailId),
    #[error(transparent)]
    Port(PortError),
}

impl From<Error> for SessionError {
    fn from(error: Error) -> Self {
        Self::Rules(error)
    }
}

pub(crate) fn apply_input_turn(
    conversation: &mut Conversation,
    turn: &InputTurn,
) -> Result<(), SessionError> {
    conversation
        .push_turn(Turn::from(turn.clone()))
        .map_err(SessionError::Rules)
}

fn existing(conversation: &Conversation, turn: &AiTurnRef) -> Result<bool, SessionError> {
    match conversation.turn(&turn.id) {
        Ok(Turn::AITurn(ai)) if ai.agent() == turn.agent && ai.author() == &turn.author => Ok(true),
        Ok(_) => Err(SessionError::WrongTurn(turn.id)),
        Err(_) => Ok(false),
    }
}

pub(crate) fn apply_entry(
    conversation: &mut Conversation,
    turn: &AiTurnRef,
    entry: TurnEntry,
) -> Result<(), SessionError> {
    if existing(conversation, turn)? {
        return conversation
            .push_entry(&turn.id, entry)
            .map_err(SessionError::Rules);
    }
    let mut created = AITurn::open(turn.id, turn.agent, turn.author.clone());
    created.push_entry(entry)?;
    conversation.push_turn(Turn::from(created))?;
    Ok(())
}

pub(crate) fn apply_closure(
    conversation: &mut Conversation,
    turn: &AiTurnRef,
    closure: TurnClosure,
) -> Result<(), SessionError> {
    if !existing(conversation, turn)? {
        return Err(SessionError::WrongTurn(turn.id));
    }
    conversation
        .push_entry(&turn.id, TurnEntry::from(closure))
        .map_err(SessionError::Rules)
}

pub(crate) fn apply_response(
    conversation: &mut Conversation,
    turn: &AiTurnRef,
    response: ModelResponse,
) -> Result<(), SessionError> {
    apply_entry(conversation, turn, TurnEntry::from(response))
}

pub(crate) fn apply_results(
    conversation: &mut Conversation,
    turn: &AiTurnRef,
    results: RuntimeToolResults,
) -> Result<(), SessionError> {
    apply_entry(conversation, turn, TurnEntry::from(results))
}
