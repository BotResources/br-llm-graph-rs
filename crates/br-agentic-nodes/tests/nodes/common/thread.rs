use br_agentic_contract::memory::{Call, Record};
use br_llm_messages::{
    AITurn, ClosureReason, Conversation, IncomingEntry, InputTurn, RuntimeToolResults, Turn,
    TurnEntry,
};

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

pub fn agent_mails(conversation: &Conversation) -> Vec<String> {
    let mut found = Vec::new();
    for turn in input_turns(conversation) {
        for entry in turn.entries() {
            if let IncomingEntry::AgentMail(mail) = entry {
                found.push(format!("{:?}", mail.content()));
            }
        }
    }
    found
}

pub fn history_text(request: &br_llm_clients_core::ClientRequest) -> String {
    format!("{:?}", request.history())
}

pub fn names(record: &Record) -> Vec<&'static str> {
    record
        .calls()
        .iter()
        .map(|call| match call {
            Call::ThreadPort => "thread_port",
            Call::AgentPort { .. } => "agent_port",
            Call::ToolsPort { .. } => "tools_port",
            Call::WorkspaceLoad => "workspace_load",
            Call::WorkspaceSave { .. } => "workspace_save",
            Call::Read { .. } => "read",
            Call::AddInputTurn { .. } => "add_input_turn",
            Call::AddEntry { .. } => "add_entry",
            Call::CloseTurn { .. } => "close_turn",
            Call::OpenResponse { .. } => "open_response",
            Call::ResponseDelta { .. } => "response_delta",
            Call::CompleteResponse { .. } => "complete_response",
            Call::DiscardResponse { .. } => "discard_response",
            Call::OpenResult { .. } => "open_result",
            Call::ResultFragment { .. } => "result_fragment",
            Call::CompleteResult { .. } => "complete_result",
            Call::DiscardResult { .. } => "discard_result",
            Call::InboxItems { .. } => "inbox_items",
            Call::InboxWait { .. } => "inbox_wait",
            Call::InboxMarkRead { .. } => "inbox_mark_read",
            Call::InboxSend { .. } => "inbox_send",
            Call::InboxDocuments { .. } => "inbox_documents",
            _ => "other",
        })
        .collect()
}
