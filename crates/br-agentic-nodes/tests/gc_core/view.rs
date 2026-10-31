use br_agentic_contract::memory::Call;
use br_llm_messages::{
    AITurn, AgentId, ContentBlock, IncomingEntry, PlainBlock, ProjectedThread, Turn, TurnEntry,
};

use crate::common::*;

pub fn role(rig: &Rig, agent: AgentId) -> &'static str {
    if agent == rig.generator {
        "generator"
    } else if agent == rig.critic {
        "critic"
    } else if agent == rig.programmatic {
        "programmatic"
    } else {
        "other"
    }
}

fn plain_text(block: &PlainBlock) -> String {
    match block {
        PlainBlock::Text(text) => text.text().as_str().to_owned(),
        _ => "?".to_owned(),
    }
}

fn incoming_text(entry: &IncomingEntry) -> String {
    match entry {
        IncomingEntry::HumanMessage(message) => {
            format!(
                "in:{}",
                message.content().iter().map(plain_text).collect::<String>()
            )
        }
        IncomingEntry::RuntimeNotice(notice) => format!("notice:{}", notice.text()),
        IncomingEntry::AgentMail(_) => "mail".to_owned(),
        _ => "in:?".to_owned(),
    }
}

fn block_text(block: &ContentBlock) -> String {
    match block {
        ContentBlock::Text(text) => text.text().as_str().to_owned(),
        ContentBlock::RuntimeToolCall(_) => "call".to_owned(),
        ContentBlock::Thinking(_) => "thinking".to_owned(),
        _ => "?".to_owned(),
    }
}

fn entry_text(entry: &TurnEntry) -> Option<String> {
    match entry {
        TurnEntry::ModelResponse(response) => Some(
            response
                .blocks()
                .iter()
                .map(block_text)
                .collect::<Vec<_>>()
                .join("&"),
        ),
        TurnEntry::RuntimeToolResults(_) => Some("results".to_owned()),
        TurnEntry::HumanMessage(message) => {
            Some(incoming_text(&IncomingEntry::from(message.clone())))
        }
        TurnEntry::RuntimeNotice(notice) => Some(format!("notice:{}", notice.text())),
        TurnEntry::TurnClosure(_) => None,
        _ => Some("?".to_owned()),
    }
}

fn closure_name(turn: &AITurn) -> String {
    match closure(turn) {
        Some(reason) => format!("{reason:?}")
            .split(' ')
            .next()
            .unwrap_or_default()
            .to_owned(),
        None => "open".to_owned(),
    }
}

pub fn timeline(rig: &Rig) -> Vec<String> {
    rig.conversation()
        .turns()
        .iter()
        .flat_map(|turn| {
            if let Some(input) = turn.as_input() {
                return input
                    .entries()
                    .iter()
                    .map(incoming_text)
                    .collect::<Vec<_>>();
            }
            let Some(ai) = turn.as_ai() else {
                return Vec::new();
            };
            let parts: Vec<String> = ai.entries().iter().filter_map(entry_text).collect();
            vec![format!(
                "{}[{}]:{}",
                role(rig, ai.agent()),
                closure_name(ai),
                parts.join("|")
            )]
        })
        .collect()
}

pub fn turns_of(rig: &Rig, who: &str) -> usize {
    let conversation = rig.conversation();
    ai_turns(&conversation)
        .iter()
        .filter(|turn| role(rig, turn.agent()) == who)
        .count()
}

pub fn count(rig: &Rig, name: &str) -> usize {
    names(&rig.record)
        .iter()
        .filter(|call| **call == name)
        .count()
}

pub fn read_for(rig: &Rig) -> Vec<AgentId> {
    rig.record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::InboxItems { context, .. } => Some(context.agent),
            _ => None,
        })
        .collect()
}

pub fn history_agent_outputs(history: &ProjectedThread) -> Vec<(AgentId, String)> {
    history
        .turns()
        .iter()
        .filter_map(Turn::as_input)
        .flat_map(|turn| turn.entries().iter())
        .filter_map(|entry| match entry {
            IncomingEntry::AgentOutput(output) => Some((
                output.agent(),
                output.content().iter().map(plain_text).collect::<String>(),
            )),
            _ => None,
        })
        .collect()
}

pub fn history_ai_turns(history: &ProjectedThread) -> Vec<(AgentId, String)> {
    history
        .turns()
        .iter()
        .filter_map(Turn::as_ai)
        .map(|turn| {
            (
                turn.agent(),
                turn.entries()
                    .iter()
                    .filter_map(entry_text)
                    .collect::<Vec<_>>()
                    .join("|"),
            )
        })
        .collect()
}
