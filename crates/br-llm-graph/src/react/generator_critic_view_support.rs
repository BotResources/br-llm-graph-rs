//! What the generator-critic tests share to read a conversation and build
//! messages: a compact view of the entries, wire messages and steps.

use br_llm_messages::{
    AssistantBlock, Conversation, Entry, Step, StopReason, Text, Turn, TurnItem, WireMessage,
    WireUserBlock,
};

/// Each entry as `user`, or `author:text` with the text of the turn's final
/// step, or `author:[n items] text` for a turn of several items.
pub(crate) fn shape(conversation: &Conversation) -> Vec<String> {
    conversation
        .entries()
        .iter()
        .map(|entry| match entry {
            Entry::UserInput(_) => "user".to_owned(),
            Entry::Turn(turn) if turn.items().len() > 1 => {
                format!(
                    "{}:[{} items] {}",
                    turn.author().unwrap(),
                    turn.items().len(),
                    turn_text(turn)
                )
            }
            Entry::Turn(turn) => format!("{}:{}", turn.author().unwrap(), turn_text(turn)),
        })
        .collect()
}

fn turn_text(turn: &Turn) -> String {
    match turn.items().last() {
        Some(TurnItem::Step(step)) => step.text().map(Text::as_str).collect::<Vec<_>>().join(" "),
        Some(TurnItem::ToolResults(_)) | None => String::new(),
    }
}

pub(crate) fn user_message(text: &str) -> WireMessage {
    WireMessage::User {
        content: vec![WireUserBlock::Text {
            text: Text::new(text).unwrap(),
        }],
    }
}

pub(crate) fn assistant_message(blocks: Vec<AssistantBlock>) -> WireMessage {
    WireMessage::Assistant { content: blocks }
}

pub(crate) fn text_block(text: &str) -> AssistantBlock {
    AssistantBlock::Text {
        text: Text::new(text).unwrap(),
    }
}

pub(crate) fn thinking_block(text: &str) -> AssistantBlock {
    AssistantBlock::Thinking(br_llm_messages::Thinking {
        text: text.to_owned(),
        signature: None,
    })
}

/// An answer whose reasoning comes before its text.
pub(crate) fn thought_step(thinking: &str, text: &str) -> Step {
    Step::new(
        vec![thinking_block(thinking), text_block(text)],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap()
}
