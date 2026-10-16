//! The two seats of the generator-critic: who writes the answers and who
//! judges them, each with what the runner gives it.

use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use br_llm_messages::{Author, Conversation};

use crate::graph::Limit;
use crate::react::given::{History, Model, Prompt};
use crate::react::round_limit::RoundLimit;
use crate::react::tool::Tool;
use crate::react::verdict::VerdictOverride;
use crate::state::StateKey;
use crate::value::{Key, NodeId};

/// The generator's seat: who writes the answers, with what, and its tools.
///
/// Its model, system and history are given as values or functions of the
/// generator's context (see `Given`): the state, the configuration, the
/// conversation, the round, the generation number and the budget. A change of
/// client never changes the seat's author.
pub struct GeneratorSeat {
    pub author: Author,
    /// The client called for each round of each generation. A schedule
    /// counts the generations, revisions not counted (`Scheduled`), and a
    /// tail takes the last generations the budget allows (`Budgeted`).
    pub model: Model<GeneratorSeat>,
    /// The generator's system text; one that resolves to a blank text gives
    /// no system text.
    pub system: Prompt<GeneratorSeat>,
    /// The messages sent. `None`: `conversation` rendered for the generator,
    /// which extends its previous request, so a provider can reuse its
    /// cache; a function gives that up.
    pub history: Option<History<GeneratorSeat>>,
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

/// What the critic reads before the answer it judges. Each default ends
/// with the answer, as one user message: `<generator_response>`, a line
/// break, the answer, a line break, `</generator_response>`, then a blank
/// line and the review request (both left out when the request resolves to a
/// blank text). That message stays a separate wire message, even after a
/// user message.
///
/// The defaults keep a stable beginning across the critic's requests, so a
/// provider can reuse its cache. A function gives that up.
pub enum CriticHistory {
    /// The entries of `conversation` before the answer, each generator turn
    /// reduced to its final step, rendered as the generator sees them
    /// (earlier answers as assistant messages, the critic's messages as
    /// framed user messages), every thinking block removed: the critic never
    /// sees the generator's tool calls, tool results or intermediate steps.
    /// An answer left with nothing, and a generation the round limit cut, are
    /// dropped, and the user messages around them merged. It keeps everything
    /// before the previous answer it judged.
    History,
    /// The entries of `conversation` up to the trigger, rendered the same
    /// way, then the answer: no earlier generation and no earlier message of
    /// the critic. It keeps the entries up to the trigger.
    Latest,
    /// A conversation state key the caller fills (an image and context only
    /// the critic reads, for example), an input of the graph, rendered for
    /// the critic, then the answer. It keeps the brief.
    Brief(StateKey<Conversation>),
    /// The messages the function gives, the answer included: the critic's
    /// context holds the answer it judges (`RequestContext::answer`).
    Given(History<CriticSeat>),
}

/// The critic's seat: who judges the answers, with what.
///
/// Its model, system, history and texts are given as values or functions of
/// the critic's context (see `Given`): the state, the configuration, the
/// conversation, the generation number, the budget, the answer it judges and
/// the attempt number. The graph reads one setting of the client the model
/// gives, at each critic call: its `ReasoningLevel`. The default verdict
/// shape asks for `thinking` exactly when that level is `Off`, so a critic
/// that does not reason natively has room to reason before judging.
pub struct CriticSeat {
    pub author: Author,
    /// The client called for each critic call. A schedule counts the
    /// critiques, verdict retries not counted (`Scheduled`), and a tail
    /// takes the last critiques the budget allows (`Budgeted`); a function
    /// may read the attempt number to give another client on a retry.
    pub model: Model<CriticSeat>,
    /// The critic's system text, always sent when the critic is called: one
    /// that resolves to a blank text then fails the critique node with
    /// `Message`, before the call. An empty answer, rejected without calling
    /// the critic, does not read it.
    pub system: Prompt<CriticSeat>,
    /// What the critic reads; by default `CriticHistory::History`.
    pub history: CriticHistory,
    /// The request that follows the wrapped answer in the critic's last
    /// message. `None`: `Please evaluate the generator's response above.` A
    /// request that resolves to a blank text leaves the wrapped answer alone.
    pub review_request: Option<Prompt<CriticSeat>>,
    /// The description of `thinking` in the default shape. `None`:
    /// `Explanation of the thought process and validation steps`.
    pub thinking_description: Option<Prompt<CriticSeat>>,
    /// The description of `is_valid` in the default shape. `None`:
    /// `Boolean indicating if the generated message is valid`.
    pub is_valid_description: Option<Prompt<CriticSeat>>,
    /// The description of `message` in the default shape. `None`:
    /// `Feedback message for the generator, empty if validation passed`.
    pub message_description: Option<Prompt<CriticSeat>>,
    /// A verdict shape of the caller's, instead of the default one; the
    /// descriptions are then ignored.
    pub verdict: Option<VerdictOverride>,
}

impl CriticSeat {
    /// A critic reading the default history, with the default request and
    /// the default verdict shape.
    pub fn new(author: Author, model: Model<CriticSeat>, system: Prompt<CriticSeat>) -> Self {
        Self {
            author,
            model,
            system,
            history: CriticHistory::History,
            review_request: None,
            thinking_description: None,
            is_valid_description: None,
            message_description: None,
            verdict: None,
        }
    }
}
