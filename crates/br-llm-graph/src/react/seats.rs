use std::sync::Arc;

use br_llm_clients_core::{Client, OutputMode};
use br_llm_messages::Author;

use crate::graph::Limit;
use crate::react::round_limit::RoundLimit;
use crate::react::source::Source;
use crate::react::tool::Tool;
use crate::react::verdict::VerdictOverride;
use crate::value::{Key, NodeId};

/// The generator's seat: who writes the answers, with what, and its tools.
///
/// The settings of `client`, its level of reasoning included, are the host's:
/// a generator that needs another model or another level is given another
/// client.
pub struct GeneratorSeat {
    pub author: Author,
    pub client: Arc<dyn Client>,
    /// The generator's system text; one that resolves to a blank text gives
    /// no system text.
    pub system: Source,
    pub output_mode: OutputMode,
    pub tools: Vec<Arc<dyn Tool>>,
    /// How the tools are split between tool nodes. Empty: one node, `tools`,
    /// runs them all; that id is the graph's own even with no tool, so a
    /// round-limit node named `tools` collides with it (`DuplicateNode`).
    pub tool_nodes: Vec<(NodeId, Vec<Arc<dyn Tool>>)>,
    /// At most that many tool calls run at once, across all the tool nodes
    /// together. `None`: every call at once.
    pub tool_concurrency: Option<Limit>,
    pub round_limit: Option<RoundLimit>,
    /// A state key holding the names of the tools offered on this run: an
    /// input of the graph, a list of strings. A call naming another tool is
    /// answered with an error result, never executed.
    pub enabled: Option<Key>,
}

/// The critic's seat: who judges the answers, with what.
///
/// The graph reads one setting of `client`: its `ReasoningLevel`. The default
/// verdict shape asks for `thinking` exactly when that level is `Off`, so a
/// critic that does not reason natively has room to reason before judging.
pub struct CriticSeat {
    pub author: Author,
    pub client: Arc<dyn Client>,
    /// The critic's system text, always sent when the critic is called: one
    /// that resolves to a blank text then fails the critique node with
    /// `Message`, before the call. An empty answer, rejected without calling
    /// the critic, does not read it.
    pub system: Source,
    /// The request that follows the wrapped answer in the critic's last
    /// message. `None`: `Please evaluate the generator's response above.` A
    /// request that resolves to a blank text leaves the wrapped answer alone.
    pub review_request: Option<Source>,
    /// The description of `thinking` in the default shape. `None`:
    /// `Explanation of the thought process and validation steps`.
    pub thinking_description: Option<Source>,
    /// The description of `is_valid` in the default shape. `None`:
    /// `Boolean indicating if the generated message is valid`.
    pub is_valid_description: Option<Source>,
    /// The description of `message` in the default shape. `None`:
    /// `Feedback message for the generator, empty if validation passed`.
    pub message_description: Option<Source>,
    /// A verdict shape of the caller's, instead of the default one; the
    /// descriptions are then ignored.
    pub verdict: Option<VerdictOverride>,
}

impl CriticSeat {
    /// A critic with the default request and the default verdict shape.
    pub fn new(author: Author, client: Arc<dyn Client>, system: Source) -> Self {
        Self {
            author,
            client,
            system,
            review_request: None,
            thinking_description: None,
            is_valid_description: None,
            message_description: None,
            verdict: None,
        }
    }
}
