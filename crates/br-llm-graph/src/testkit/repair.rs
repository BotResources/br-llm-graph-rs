//! The repair of a failed or cancelled turn, as a real store makes it: every
//! tool call without a result is answered as interrupted, and an unfinished
//! step is kept when br-llm-messages can finish it, else dropped.

use br_llm_messages::{
    Step, StepDraft, StopReason, StreamEvent, Text, ToolResult, ToolResultBlock, Turn, TurnItem,
    TurnState,
};

use crate::testkit::store::TurnRecord;

/// The text of the error result a repaired turn answers an interrupted call
/// with.
pub const INTERRUPTED: &str = "The call was interrupted: its result was not recorded.";

/// The stop reason of a step the repair finishes when it calls no tool.
pub const INTERRUPTED_STOP: &str = "interrupted";

/// Repairs `record`, so the thread always renders.
pub(crate) fn repair(record: &mut TurnRecord) {
    if let Some(draft) = record.draft.take()
        && let Some(step) = finish(&draft.events)
    {
        match &mut record.turn {
            // A step the turn refuses is dropped, as one that cannot finish.
            Some(turn) => drop(turn.push_step(step)),
            None => {
                record.turn = Some(Turn::new(
                    record.id.clone(),
                    Some(record.author.clone()),
                    step,
                ));
            }
        }
    }
    if let Some(turn) = &mut record.turn {
        answer_interrupted(turn);
    }
}

/// The step `events` make once its open block is closed and a finish is
/// added: awaiting tool results when it calls a tool, else stopped as
/// interrupted. `None` when br-llm-messages cannot finish it (a cut tool
/// call's arguments, an empty reply...).
fn finish(events: &[StreamEvent]) -> Option<Step> {
    let mut draft = StepDraft::new();
    let mut open = None;
    let mut calls = false;
    for event in events {
        draft.apply(event.clone()).ok()?;
        match event {
            StreamEvent::BlockStart { index, .. } => open = Some(*index),
            StreamEvent::ToolCallStart { index, .. } => {
                open = Some(*index);
                calls = true;
            }
            StreamEvent::BlockEnd { .. } => open = None,
            _ => {}
        }
    }
    if let Some(index) = open {
        draft.apply(StreamEvent::BlockEnd { index }).ok()?;
    }
    let stop_reason = if calls {
        StopReason::AwaitingToolResults
    } else {
        StopReason::Other {
            reason: INTERRUPTED_STOP.to_owned(),
        }
    };
    draft
        .apply(StreamEvent::Finish {
            stop_reason,
            usage: None,
        })
        .ok()?;
    draft.finish(None).ok()
}

/// Answers each pending call of `turn` with the interrupted error result.
fn answer_interrupted(turn: &mut Turn) {
    let TurnState::AwaitingToolResults { pending } = turn.state() else {
        return;
    };
    let calls: Vec<_> = turn
        .items()
        .iter()
        .rev()
        .find_map(|item| match item {
            TurnItem::Step(step) => Some(step),
            TurnItem::ToolResults(_) => None,
        })
        .into_iter()
        .flat_map(Step::tool_calls)
        .filter(|call| pending.contains(&call.id))
        .cloned()
        .collect();
    let text = Text::new(INTERRUPTED).expect("a text");
    for call in calls {
        let result = ToolResult::new(
            call.id,
            call.name,
            vec![ToolResultBlock::text(text.clone())],
            true,
        );
        drop(turn.push_result(result));
    }
}
