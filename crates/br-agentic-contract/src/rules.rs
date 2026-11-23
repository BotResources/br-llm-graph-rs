use br_llm_messages::{
    AITurn, Conversation, Error, InputTurn, ModelResponse, RuntimeToolResults, Turn, TurnClosure,
    TurnEntry, TurnId,
};

use crate::thread_access::AiTurnRef;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ApplyError {
    #[error("the rules of br-llm-messages refuse the change: {0}")]
    Rules(#[source] Error),
    #[error("the turn {0} is not the AI turn of that agent and author")]
    WrongTurn(TurnId),
}

impl From<Error> for ApplyError {
    fn from(error: Error) -> Self {
        Self::Rules(error)
    }
}

pub fn apply_input_turn(
    conversation: &mut Conversation,
    turn: &InputTurn,
) -> Result<(), ApplyError> {
    conversation
        .push_turn(Turn::from(turn.clone()))
        .map_err(ApplyError::Rules)
}

fn existing(conversation: &Conversation, turn: &AiTurnRef) -> Result<bool, ApplyError> {
    match conversation.turn(&turn.id) {
        Ok(Turn::AITurn(ai)) if ai.agent() == turn.agent && ai.author() == &turn.author => Ok(true),
        Ok(_) => Err(ApplyError::WrongTurn(turn.id)),
        Err(_) => Ok(false),
    }
}

pub fn apply_entry(
    conversation: &mut Conversation,
    turn: &AiTurnRef,
    entry: TurnEntry,
) -> Result<(), ApplyError> {
    if existing(conversation, turn)? {
        return conversation
            .push_entry(&turn.id, entry)
            .map_err(ApplyError::Rules);
    }
    let mut created = AITurn::open(turn.id, turn.agent, turn.author.clone());
    created.push_entry(entry)?;
    conversation.push_turn(Turn::from(created))?;
    Ok(())
}

pub fn apply_closure(
    conversation: &mut Conversation,
    turn: &AiTurnRef,
    closure: TurnClosure,
) -> Result<(), ApplyError> {
    if !existing(conversation, turn)? {
        return Err(ApplyError::WrongTurn(turn.id));
    }
    conversation
        .push_entry(&turn.id, TurnEntry::from(closure))
        .map_err(ApplyError::Rules)
}

pub fn apply_response(
    conversation: &mut Conversation,
    turn: &AiTurnRef,
    response: ModelResponse,
) -> Result<(), ApplyError> {
    apply_entry(conversation, turn, TurnEntry::from(response))
}

pub fn apply_results(
    conversation: &mut Conversation,
    turn: &AiTurnRef,
    results: RuntimeToolResults,
) -> Result<(), ApplyError> {
    apply_entry(conversation, turn, TurnEntry::from(results))
}
