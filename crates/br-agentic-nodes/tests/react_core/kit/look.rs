use br_agentic_contract::memory::{Call, Record};
use br_llm_clients_core::ClientRequest;
use br_llm_messages::{ClosureReason, Conversation, EntryId, ModelResponse, Turn, TurnEntry};

use crate::common::{ai_turns, names};

pub fn count(record: &Record, name: &str) -> usize {
    names(record).iter().filter(|seen| **seen == name).count()
}

pub fn position(record: &Record, name: &str) -> usize {
    names(record)
        .iter()
        .position(|seen| *seen == name)
        .unwrap_or_else(|| panic!("the record holds no {name}"))
}

pub fn last_position(record: &Record, name: &str) -> usize {
    names(record)
        .iter()
        .rposition(|seen| *seen == name)
        .unwrap_or_else(|| panic!("the record holds no {name}"))
}

pub fn warning_of(request: &ClientRequest) -> Option<String> {
    request
        .history()
        .turns()
        .iter()
        .filter_map(Turn::as_ai)
        .flat_map(|turn| turn.entries().iter())
        .find_map(|entry| match entry {
            TurnEntry::RuntimeNotice(notice) => Some(notice.text().to_owned()),
            _ => None,
        })
}

pub fn all_responses(conversation: &Conversation) -> Vec<ModelResponse> {
    ai_turns(conversation)
        .into_iter()
        .flat_map(|turn| turn.entries().iter())
        .filter_map(|entry| match entry {
            TurnEntry::ModelResponse(response) => Some(response.clone()),
            _ => None,
        })
        .collect()
}

pub fn response_ids(conversation: &Conversation) -> Vec<EntryId> {
    all_responses(conversation)
        .iter()
        .map(ModelResponse::id)
        .collect()
}

pub fn completed_tools(record: &Record) -> Vec<String> {
    record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::CompleteResult { result, .. } => result
                .results()
                .first()
                .map(|found| found.tool_name().as_str().to_owned()),
            _ => None,
        })
        .collect()
}

pub fn opened_responses(record: &Record) -> Vec<EntryId> {
    record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::OpenResponse { entry, .. } => Some(*entry),
            _ => None,
        })
        .collect()
}

pub fn discarded_responses(record: &Record) -> Vec<EntryId> {
    record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::DiscardResponse { entry, .. } => Some(*entry),
            _ => None,
        })
        .collect()
}

pub fn completed_responses(record: &Record) -> Vec<EntryId> {
    record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::CompleteResponse { response, .. } => Some(response.id()),
            _ => None,
        })
        .collect()
}

pub fn closures_given(record: &Record) -> Vec<ClosureReason> {
    record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::CloseTurn { closure, .. } => Some(closure.reason().clone()),
            _ => None,
        })
        .collect()
}

pub fn deltas_given(record: &Record) -> Vec<EntryId> {
    record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::ResponseDelta { entry, .. } => Some(*entry),
            _ => None,
        })
        .collect()
}

pub fn opened_results(record: &Record) -> Vec<EntryId> {
    record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::OpenResult { entry, .. } => Some(*entry),
            _ => None,
        })
        .collect()
}

pub fn completed_result_ids(record: &Record) -> Vec<EntryId> {
    record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::CompleteResult { result, .. } => Some(result.id()),
            _ => None,
        })
        .collect()
}

pub fn result_fragments(record: &Record) -> Vec<(EntryId, String)> {
    record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::ResultFragment { entry, fragment } => Some((*entry, fragment.clone())),
            _ => None,
        })
        .collect()
}

pub fn discarded_results(record: &Record) -> Vec<EntryId> {
    record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::DiscardResult { entry, .. } => Some(*entry),
            _ => None,
        })
        .collect()
}

pub fn closed_turn_ids(record: &Record) -> Vec<br_llm_messages::TurnId> {
    record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::CloseTurn { turn, .. } => Some(turn.id),
            _ => None,
        })
        .collect()
}

pub fn saves(record: &Record) -> Vec<Vec<br_agentic_contract::WorkspaceFile>> {
    record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::WorkspaceSave { files } => Some(files.clone()),
            _ => None,
        })
        .collect()
}

pub fn agent_labels(record: &Record) -> Vec<String> {
    record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::AgentPort { label, .. } => Some(label.as_str().to_owned()),
            _ => None,
        })
        .collect()
}
