//! What the store does to an agent's turn: it folds the events of each step
//! into it, and repairs it when it is failed or cancelled, as the thread
//! store's port asks for it: an unfinished step is kept when br-llm-messages
//! can finish it, else dropped, and every tool call without a result is
//! answered as interrupted, so the thread always renders.

use br_llm_messages::{
    Step, StepDraft, StopReason, StreamEvent, Text, ToolResult, ToolResultBlock, Turn, TurnItem,
    TurnState,
};

use super::store::TurnRecord;

/// Folds `event` into the step under way; at its finish, the step joins the
/// turn.
pub fn fold(record: &mut TurnRecord, event: &StreamEvent) -> Result<(), String> {
    let draft = record.draft.get_or_insert_with(StepDraft::new);
    draft.apply(event.clone()).map_err(|e| e.to_string())?;
    record.events.push(event.clone());
    if !matches!(event, StreamEvent::Finish { .. }) {
        return Ok(());
    }
    record.events.clear();
    let step = record
        .draft
        .take()
        .map(|draft| draft.finish(None))
        .transpose()
        .map_err(|e| e.to_string())?;
    match (&mut record.turn, step) {
        (Some(turn), Some(step)) => turn.push_step(step).map_err(|e| e.to_string()),
        (None, Some(step)) => {
            record.turn = Some(Turn::new(
                record.id.clone(),
                Some(record.author.clone()),
                step,
            ));
            Ok(())
        }
        (_, None) => Ok(()),
    }
}

/// The error result a repaired turn answers an interrupted call with.
const INTERRUPTED: &str = "The call was interrupted: its result was not recorded.";

/// Repairs `record`.
pub fn repair(record: &mut TurnRecord) {
    let events = std::mem::take(&mut record.events);
    if let Some(step) = finish(&events) {
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

/// The step `events` make once the open block is closed and a finish added:
/// awaiting tool results when it calls a tool, else stopped as interrupted.
/// `None` when br-llm-messages cannot finish it (no event, a cut tool call's
/// arguments...).
fn finish(events: &[StreamEvent]) -> Option<Step> {
    if events.is_empty() {
        return None;
    }
    let mut draft = StepDraft::new();
    let (mut open, mut calls) = (None, false);
    for event in events {
        draft.apply(event.clone()).ok()?;
        match event {
            StreamEvent::BlockStart { index, .. } => open = Some(*index),
            StreamEvent::ToolCallStart { index, .. } => (open, calls) = (Some(*index), true),
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
            reason: "interrupted".to_owned(),
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
    let text = Text::new(INTERRUPTED).expect("a text that is not blank");
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
