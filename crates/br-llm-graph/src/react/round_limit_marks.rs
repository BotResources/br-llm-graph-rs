//! The marks the round limit's node leaves in a turn: the refusal it
//! answers each call with and the step it closes a cut turn with, and how a
//! turn is read for them.

use br_llm_messages::{AssistantBlock, StopReason, ToolResult, ToolResultBlock, Turn, TurnItem};

/// The stop reason of the step that closes a turn cut by `OnLimit::End`.
pub(crate) const LIMIT_REACHED: &str = "tool_round_limit";

/// The first and last parts of the text of the step that closes a turn cut
/// by `OnLimit::End`; the limit, a number, stands between them.
const CLOSING_HEAD: &str = "Tool round limit reached (";
const CLOSING_TAIL: &str = "): no final reply.";

/// The text of the step that closes a turn cut by `OnLimit::End`.
pub(crate) fn closing(max: usize) -> String {
    format!("{CLOSING_HEAD}{max}{CLOSING_TAIL}")
}

/// Whether `turn` is a turn `OnLimit::End` cut: its final step is the step
/// the limit node closes it with and nothing else (the stop reason
/// `other(tool_round_limit)`, one text block holding the closing text, no
/// model and no usage), and the item before it holds only the limit's
/// refusals.
///
/// A client's step carries a model (br-llm-clients-core), but a step read
/// back from a thread does not: a store folds stream events, which carry
/// none. So the step alone cannot tell a provider's reply that keeps this
/// stop reason verbatim from the limit's closing step once a durable agent
/// rehydrates; the refusals before it can, since tool nodes never answer a
/// round with the limit's refusal alone. Callers ask it only of a generator
/// whose round limit is `End`.
pub(crate) fn cut_by_limit(turn: &Turn) -> bool {
    let [.., TurnItem::ToolResults(results), TurnItem::Step(step)] = turn.items() else {
        return false;
    };
    let closes = matches!(step.stop_reason(), StopReason::Other { reason } if reason == LIMIT_REACHED)
        && step.model().is_none()
        && step.usage().is_none()
        && matches!(step.content(), [AssistantBlock::Text { text }] if is_closing(text.as_str()));
    closes && refusals_only(results.results())
}

/// Whether `text` is the closing text of some limit, written as the node
/// writes it.
fn is_closing(text: &str) -> bool {
    text.strip_prefix(CLOSING_HEAD)
        .and_then(|rest| rest.strip_suffix(CLOSING_TAIL))
        .and_then(|max| max.parse::<usize>().ok())
        .is_some_and(|max| closing(max) == text)
}

/// Whether `results` is a round the limit node answered: not empty, and
/// every result its refusal.
fn refusals_only(results: &[ToolResult]) -> bool {
    !results.is_empty() && results.iter().all(is_refusal)
}

/// Whether the round limit was reached in `turn` under a limit of `max`: a
/// round at 0-based index `max` or more holds only its refusals. Tool nodes
/// never run past the limit, so only the limit node answers those rounds.
pub(crate) fn reached_in(turn: &Turn, max: usize) -> bool {
    turn.items()
        .iter()
        .filter_map(|item| match item {
            TurnItem::ToolResults(results) => Some(results.results()),
            TurnItem::Step(_) => None,
        })
        .skip(max)
        .any(refusals_only)
}

/// The first and last parts of the error result the limit node answers a
/// call with; the limit, a number, stands between them.
const REFUSAL_HEAD: &str = "Tool round limit reached (";
const REFUSAL_TAIL: &str = "): this call was not executed. Reply without calling tools.";

/// The error result's text the limit node answers a call with.
pub(crate) fn refusal(max: usize) -> String {
    format!("{REFUSAL_HEAD}{max}{REFUSAL_TAIL}")
}

/// Whether `result` is the limit node's answer to a call: an error result
/// whose one text is the limit's refusal, whatever limit it names, written
/// as the node writes it (no sign, no leading zero).
fn is_refusal(result: &ToolResult) -> bool {
    result.is_error
        && matches!(result.content.as_slice(), [ToolResultBlock::Text { text }] if text
            .as_str()
            .strip_prefix(REFUSAL_HEAD)
            .and_then(|rest| rest.strip_suffix(REFUSAL_TAIL))
            .and_then(|max| max.parse::<usize>().ok())
            .is_some_and(|max| refusal(max) == text.as_str()))
}
