use br_llm_clients_core::{ClientRequest, OutputMode};
use br_llm_messages::{
    AgentId, EntryId, HumanId, IncomingEntry, PlainBlock, ProjectedThread, Turn, TurnEntry,
};

use crate::common::text_of;

#[derive(Debug, Clone, PartialEq)]
pub enum Seen {
    Human {
        entry: EntryId,
        human: HumanId,
        text: String,
    },
    Output {
        agent: AgentId,
        author: String,
        text: String,
    },
    Mail,
    Notice,
    Answer {
        text: String,
    },
    Other,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SeenTurn {
    pub ai_agent: Option<AgentId>,
    pub entries: Vec<Seen>,
}

pub fn plain_text(blocks: &[PlainBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            PlainBlock::Text(text) => Some(text.text().as_str().to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn incoming(entry: &IncomingEntry) -> Seen {
    match entry {
        IncomingEntry::HumanMessage(message) => Seen::Human {
            entry: message.id(),
            human: message.human(),
            text: plain_text(message.content()),
        },
        IncomingEntry::AgentOutput(output) => Seen::Output {
            agent: output.agent(),
            author: output.author().as_str().to_owned(),
            text: plain_text(output.content()),
        },
        IncomingEntry::AgentMail(_) => Seen::Mail,
        IncomingEntry::RuntimeNotice(_) => Seen::Notice,
        _ => Seen::Other,
    }
}

pub fn seen(history: &ProjectedThread) -> Vec<SeenTurn> {
    history
        .turns()
        .iter()
        .map(|turn| match turn {
            Turn::InputTurn(input) => SeenTurn {
                ai_agent: None,
                entries: input.entries().iter().map(incoming).collect(),
            },
            Turn::AITurn(ai) => SeenTurn {
                ai_agent: Some(ai.agent()),
                entries: ai
                    .entries()
                    .iter()
                    .map(|entry| match entry {
                        TurnEntry::ModelResponse(response) => Seen::Answer {
                            text: text_of(response),
                        },
                        _ => Seen::Other,
                    })
                    .collect(),
            },
            _ => SeenTurn {
                ai_agent: None,
                entries: Vec::new(),
            },
        })
        .collect()
}

pub fn seen_by(request: &ClientRequest) -> Vec<SeenTurn> {
    seen(request.history())
}

pub fn last_human(request: &ClientRequest) -> (EntryId, HumanId, String) {
    let turns = seen_by(request);
    match turns.last().and_then(|turn| turn.entries.last()) {
        Some(Seen::Human { entry, human, text }) => (*entry, *human, text.clone()),
        other => panic!("the projection does not end with a human message: {other:?}"),
    }
}

pub fn shape_names(request: &ClientRequest) -> Vec<String> {
    match request.output() {
        OutputMode::Structured(shape) => shape
            .parameters()
            .iter()
            .map(|parameter| parameter.name().to_owned())
            .collect(),
        other => panic!("the request does not ask for a structured reply: {other:?}"),
    }
}
