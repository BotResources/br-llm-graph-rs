use br_agentic_contract::AgentLabel;
use br_graph_contract::Fields;
use br_llm_messages::{AgentId, Author, PlainText};

use crate::config::AuthorSource;
use crate::fault::Fault;

pub(crate) struct SeatSpec<'b, F: Fields> {
    pub(crate) label: &'static str,
    pub(crate) author: &'b AuthorSource<F>,
    pub(crate) prompt: bool,
}

pub(crate) struct Seat {
    pub(crate) label: AgentLabel,
    pub(crate) agent: AgentId,
    pub(crate) author: Author,
    pub(crate) system: Option<PlainText>,
}

pub(crate) struct Seats(pub(crate) Vec<Seat>);

impl Seats {
    pub(crate) fn main(&self) -> Result<&Seat, Fault> {
        self.0
            .first()
            .ok_or_else(|| Fault::Value("the node has no agent".to_owned()))
    }

    pub(crate) fn get(&self, label: &str) -> Result<&Seat, Fault> {
        self.0
            .iter()
            .find(|seat| seat.label.as_str() == label)
            .ok_or_else(|| Fault::Value(format!("the node has no agent {label}")))
    }
}
