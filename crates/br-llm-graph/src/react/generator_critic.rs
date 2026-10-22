use std::sync::Arc;

use crate::graph::{Count, Node};
use crate::react::given::Prompt;
use crate::react::rule_check::RuleCheck;
use crate::react::seats::{CriticSeat, GeneratorSeat};
use crate::react::thread_store::ThreadStore;
use crate::state::Kind;
use crate::value::Key;

/// A generator and a critic writing in one conversation, as a graph, to call
/// with `SubGraph` or run on its own.
///
/// # Seats and keys
///
/// Each seat's model, system and history are given as values or functions of
/// its context (see `Given`, `RequestContext`), which holds the state, the
/// configuration and the attempt number: the generator's adds the
/// conversation, the round, the generation number and the budget; the
/// critic's the conversation, the generation number, the budget and the
/// answer it judges. A change of client never changes a seat's author; two
/// seats with one author are refused (`SameAuthor`).
///
/// - Inputs: `opening` (conversation), what the caller adds before the
///   generator answers, typically one user input; durable, user inputs
///   ending with the trigger, the user input the call answers
///   (`OpeningNotInputs` otherwise). Durable, `thread` (string), as for a
///   `ReactAgent`. Each state key a function declares, of its kind; the
///   generator's `enabled` key (list of strings) when set; each key of
///   `inputs`, for the generator's tools and the functions to read.
/// - Outputs, plain data: `answer` (string, the generator's last answer of
///   this call as the critic sees it: the JSON text of its structured block
///   when it has one, else its text blocks joined verbatim; empty when none,
///   or when the round limit cut that generation), `is_validated` (bool),
///   `nb_generations` (int, the generator's turns of this call, revisions
///   included), the generator's round-limit flag when it has one (true when
///   the limit was reached in at least one generation of this call) and,
///   durable, `thread` (string). `conversation` is private: neither an input
///   nor an output, never part of a checkpoint of the graph, so a run of it
///   resumed from a checkpoint enters again through its entry.
/// - Configuration: the keys of the budget, the revision limit and the
///   verdict retries, the generator's reply retries, tool concurrency and
///   round limit, and each configuration key a function declares, of its
///   kind. The edge of `route` resolves the counts and the limits, in that
///   order, before anything is routed: a refused one (`CountNegative`,
///   `LimitNotPositive`) is the run's error itself, and no model is called.
///
/// One key given twice with one kind is declared once (a key several
/// functions read, a key a function reads that is also `enabled`, a key of
/// `inputs` or the key of a count or a limit). A key given with another kind
/// than the graph already has is refused (`KeyCollision`), and so is a key
/// the graph keeps for itself: `opening`, `answer`, `is_validated`,
/// `nb_generations`, `conversation`, its private keys `turn_rank` and
/// `call_turns`, `thread` when durable and the round limit's flag. A
/// generator round-limit flag that names `is_validated`, `opening`, `thread`
/// or `conversation` is refused (`KeyCollision`) first.
///
/// The graph's own nodes are `route`, `generate`, `check`, `critique`,
/// `finish`, `prepare` when set and, without a split, `tools`, even when the
/// generator has no tool: a tool node or a round-limit node of the generator
/// of one of these ids collides with it (`DuplicateNode`). A call of the
/// generator that names a tool no tool node runs is answered with an error
/// result, never executed, as in a ReAct agent (see `ReactAgent`). The run
/// ends with `validated` when `is_validated` is true, else `exhausted`.
///
/// # What it records
///
/// The generator-critic has one conversation, `conversation`, which is its
/// thread when durable (see `ThreadStore`): the opening, every generation
/// with its tool traffic and thinking, and every message of the critic. The
/// critic works in no thread and keeps no history of its own. A message of
/// the critic is a new turn of the critic holding one text step, told apart
/// by its stop reason: a critique (`end_turn`), a rejection by the critic or
/// of an empty answer; a revision (`other(rule_check)`), asked by the rule
/// check; an acceptance (`other(accepted)`), holding the verdict's message
/// or `Accepted.` when it is blank. Nothing of the critic's reasoning, and
/// nothing of a verdict retry, is recorded.
///
/// # Routing
///
/// With `prepare`, the graph enters through it, once, then `route`, which
/// starts `conversation` from `opening` (from the thread when durable) and
/// routes on the last turn of this call: in memory, the last turn of the
/// generator or the critic since the trigger; durable, the turns the thread
/// holds under the ids of this call. A completed turn of the generator goes
/// to the check; a failed or cancelled one is left out, and the call is
/// routed on the turn before it. A critique or a revision is routed as
/// after it; an acceptance finishes `validated` without calling a model;
/// nothing goes to the generator.
///
/// Durable, a turn found open on entry is failed and repaired by the store
/// (see `ThreadStore`), then routed by its kind. A turn of the generator the
/// thread holds failed or cancelled (cut by a crash, failed under a reply
/// asked for again, or ended so by the host) is left out: it stays in the
/// conversation, is neither judged nor counted, and the call is routed on
/// the turn before it, so the generator answers again in the turn of the
/// next rank; a reply asked for again after its turn was failed is made
/// there, inside the same generation. A message of the critic is a
/// judgment only with one of the three stop reasons above: a repaired one
/// (cut while it was written, with another stop reason or no step) is not
/// one of the call's turns, so the critic judges the same answer again,
/// exactly as an uninterrupted call would, and it is not counted against
/// the budget.
///
/// A generation is the ReAct loop of the generator on `conversation`; each
/// opens a new generator turn, so the round limit counts per generation.
/// With a structured output mode, the generator's final reply is checked
/// against its shape, as a ReAct agent's is, before the check below. A
/// reply that cannot be used (see `GeneratorSeat`) is asked for again
/// within the generator's reply retries, inside the same generation: a
/// retry is no generation, no revision and no critique. When no retry is
/// left, the run fails (`Model` or `Structured`, inside the `NodeFailed` of
/// `generate`). The check judges each generation before the critic:
/// - an empty answer (its final step holds no non-blank text and no
///   structured block, or a generation the round limit cut with `End`) is
///   rejected without the critic: where the critic would judge it, a
///   critique holding `empty_answer_critique` is recorded and the run goes on
///   as after a rejection; an empty answer that is not judged ends
///   `exhausted` with nothing recorded;
/// - otherwise the rule check, when set, passes it, asks for a revision, or
///   fails the run with its error, as returned. A revision is recorded and
///   the generator answers again; a generation has at most `revision_limit`
///   revisions in a row, after which the answer goes on as if it had passed.
///
/// After the check, when the critiques have reached the budget the run
/// finishes without the critic; otherwise the critic judges. A valid verdict
/// records an acceptance and finishes `validated`. A rejection records a
/// critique; the generator answers again while the critiques are below the
/// budget, or once more, unassessed, when `final_generation` and the
/// critiques equal the budget; otherwise the run finishes `exhausted` on the
/// rejected answer. The budget counts critiques only, revisions not. The
/// same rules route a turn found on entry, a revision included (the
/// generator answers again, whatever the budget). A budget of 0 gives one
/// generation, with its revisions, and no critic call.
///
/// # What each seat reads
///
/// The generator: its system text (none when blank), its history, by
/// default `conversation` rendered for it (the opening, its earlier
/// generations with their tool traffic and thinking, the critic's messages
/// as framed user messages), its tools filtered by `enabled`, and its output
/// mode. Its stream events carry the key `conversation`.
///
/// The critic: its system text, always sent (a text that resolves to a blank
/// one fails the node with `Message`), what it reads (see `CriticHistory`),
/// no tools, and the structured mode with the verdict shape. Its stream
/// events carry the key `critic`.
///
/// # Verdict
///
/// The default shape, `verdict`, has `thinking` (string), `is_valid`
/// (boolean) and `message` (string), in that order, all required; `thinking`
/// is present exactly when the `ReasoningLevel` of the client the critic's
/// model gives for that call is `Off`. Each description is the seat's, given
/// at each critic call, or its default (see `CriticSeat`). An override sends
/// the caller's shape (see `VerdictOverride`), read by its reader when it
/// has one. A missing structured block, a block that does not read as the
/// default verdict, a block the reader says it cannot read
/// (`Reading::Unreadable`), a rejection whose message is blank, or a reply
/// the client refuses as unusable (`ReplyUnusable`) or malformed
/// (`MalformedResponse`, which a conforming client gives for a reply
/// without a structured block) is an unreadable verdict: while
/// `verdict_retries` are left, the critic is asked again, its request
/// followed by a user message `Your reply could not be read as a verdict:
/// {reason}. Reply again with the verdict.`, the attempt number of its
/// context one higher; after the last retry the run fails with `Structured`
/// and no critique is recorded. Retries are not recorded and do not count
/// against the budget.
pub struct GeneratorCritic {
    /// Who writes the answers.
    pub generator: GeneratorSeat,
    /// Who judges them.
    pub critic: CriticSeat,
    /// The critique budget.
    pub max_critiques: Count,
    /// Whether the generator answers once more, unassessed, after the last
    /// critique the budget allows. Default: true.
    pub final_generation: bool,
    /// The revisions in a row a generation may have. Default: 2.
    pub revision_limit: Count,
    /// The times an unreadable verdict is asked again. Default: 0.
    pub verdict_retries: Count,
    /// Extra state keys the caller maps per run, read by the generator's
    /// tools and the functions.
    pub inputs: Vec<(Key, Kind)>,
    /// A judgment of each generation made by code, before the critic.
    pub rule_check: Option<RuleCheck>,
    /// The critique that rejects an empty answer, written without calling
    /// the critic. `None`, or a text that resolves blank: `The answer was
    /// empty. Write a complete answer.`
    pub empty_answer_critique: Option<Prompt<GeneratorCritic>>,
    /// A node run once, as the graph's entry, before the routing: it may do
    /// I/O (it holds its capabilities as fields) and writes through ordinary
    /// updates, typically into `opening`. It is checked through
    /// `Node::check` like any node; its failure fails the graph. A run
    /// resumed from a checkpoint enters through it again.
    pub prepare: Option<Box<dyn Node>>,
    /// The store of the generator-critic's threads. `None`: it works in
    /// memory.
    pub thread_store: Option<Arc<dyn ThreadStore>>,
}

impl GeneratorCritic {
    /// A generator-critic of `generator` and `critic` with the critique
    /// budget `max_critiques`, in memory, with the defaults.
    pub fn new(generator: GeneratorSeat, critic: CriticSeat, max_critiques: Count) -> Self {
        Self {
            generator,
            critic,
            max_critiques,
            final_generation: true,
            revision_limit: Count::Fixed(2),
            verdict_retries: Count::Fixed(0),
            inputs: Vec::new(),
            rule_check: None,
            empty_answer_critique: None,
            prepare: None,
            thread_store: None,
        }
    }
}
