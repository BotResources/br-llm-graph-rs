use br_agentic_contract::ThreadRef;
use br_agentic_contract::memory::MemoryHub;
use br_llm_messages::{
    AITurn, ClosureReason, Conversation, InputTurn, RuntimeToolResults, Turn, TurnEntry,
};

pub fn conversation(hub: &MemoryHub, thread: &ThreadRef) -> Conversation {
    hub.conversation(thread).unwrap()
}

pub fn ai_turns(conversation: &Conversation) -> Vec<&AITurn> {
    conversation
        .turns()
        .iter()
        .filter_map(Turn::as_ai)
        .collect()
}

pub fn input_turns(conversation: &Conversation) -> Vec<&InputTurn> {
    conversation
        .turns()
        .iter()
        .filter_map(Turn::as_input)
        .collect()
}

pub fn closure(turn: &AITurn) -> Option<ClosureReason> {
    turn.state().closure().cloned()
}

pub fn responses(turn: &AITurn) -> Vec<String> {
    turn.entries()
        .iter()
        .filter_map(|entry| match entry {
            TurnEntry::ModelResponse(response) => Some(super::text_of(response)),
            _ => None,
        })
        .collect()
}

pub fn results(turn: &AITurn) -> Vec<&RuntimeToolResults> {
    turn.entries()
        .iter()
        .filter_map(|entry| match entry {
            TurnEntry::RuntimeToolResults(results) => Some(results),
            _ => None,
        })
        .collect()
}

pub fn kinds(turn: &AITurn) -> Vec<String> {
    turn.entries()
        .iter()
        .map(|entry| format!("{:?}", entry.kind()))
        .collect()
}

pub fn history_text(request: &br_llm_clients_core::ClientRequest) -> String {
    format!("{:?}", request.history())
}

pub fn texts_of_results(turn: &AITurn) -> Vec<(bool, String)> {
    results(turn)
        .into_iter()
        .flat_map(|entry| entry.results())
        .map(|result| {
            let text = result
                .content()
                .iter()
                .map(|block| format!("{block:?}"))
                .collect::<Vec<_>>()
                .join("");
            (result.is_error(), text)
        })
        .collect()
}

pub fn agent_mails(conversation: &Conversation) -> Vec<(String, String)> {
    input_turns(conversation)
        .into_iter()
        .flat_map(|turn| turn.entries())
        .filter_map(|entry| match entry {
            br_llm_messages::IncomingEntry::AgentMail(mail) => Some((
                mail.author().as_str().to_owned(),
                format!("{:?}", mail.content()),
            )),
            _ => None,
        })
        .collect()
}
