use std::sync::Arc;

use br_agentic_contract::{FileSystemMode, WorkspaceFile};
use br_agentic_nodes::AuthorSource;
use br_llm_messages::{ClosureReason, IncomingEntry, InputTurn, ResponseEnding, Turn, TurnEntry};

use crate::common::*;
use crate::kit::*;

fn file(path: &str, content: &str) -> WorkspaceFile {
    WorkspaceFile {
        path: path.to_owned(),
        content: content.as_bytes().to_vec(),
    }
}

fn interrupted(reason: &Option<ClosureReason>) -> bool {
    matches!(reason, Some(ClosureReason::Interrupted { .. }))
}

#[tokio::test(start_paused = true)]
async fn given_an_open_turn_of_the_agent_with_an_orphan_call_when_the_instance_starts_then_it_closes_interrupted_and_adds_no_result()
 {
    let rig = seeded(|rig, conversation| {
        let call = vec![Part::Call("alpha".to_owned(), "{}".to_owned())];
        seed_turn(
            conversation,
            rig.agent,
            call,
            ResponseEnding::tool_use(),
            None,
        );
    });
    let model = Model::new(vec![Act::text("fresh")]);
    let node = node(&rig, plains(&[&model]));
    let old = ai_turns(&rig.conversation())[0].id();

    let update = Driver::result(&node).await;

    let conversation = rig.conversation();
    let turns = ai_turns(&conversation);
    assert_eq!(turns.len(), 2);
    assert!(interrupted(&closure(turns[0])));
    assert!(results(turns[0]).is_empty());
    assert_eq!(count(&rig.record, "complete_result"), 0);
    assert_eq!(count(&rig.record, "add_entry"), 0);
    assert_eq!(closed_turn_ids(&rig.record)[0], old);
    assert_eq!(update.text, "fresh");
    assert_ne!(turns[1].id(), old);
    assert_eq!(turns[1].id(), model.requests()[0].reply_turn());
}

#[tokio::test(start_paused = true)]
async fn given_an_interrupted_turn_when_the_first_step_runs_then_the_projection_of_the_copy_holds_the_turn_as_closed()
 {
    let rig = seeded(|rig, conversation| {
        seed_turn(
            conversation,
            rig.agent,
            text_part("unfinished"),
            ResponseEnding::provider_continuation(),
            None,
        );
    });
    let model = Model::new(vec![Act::text("fresh")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    let request = &model.requests()[0];
    let held: Vec<_> = request
        .history()
        .turns()
        .iter()
        .filter_map(Turn::as_ai)
        .map(|turn| turn.state().closure().cloned())
        .collect();
    assert_eq!(held.len(), 1);
    assert!(interrupted(&held[0]));
    assert!(history_text(request).contains("start"));
}

#[tokio::test(start_paused = true)]
async fn given_open_turns_of_the_agent_and_of_another_agent_when_the_instance_starts_then_only_those_of_the_agent_are_closed()
 {
    let rig = seeded(|rig, conversation| {
        seed_turn(
            conversation,
            rig.agent,
            text_part("mine"),
            ResponseEnding::provider_continuation(),
            None,
        );
        seed_turn(
            conversation,
            agent_id(),
            text_part("theirs"),
            ResponseEnding::provider_continuation(),
            None,
        );
    });
    let model = Model::new(vec![Act::text("fresh")]);
    let node = node(&rig, plains(&[&model]));
    let other = ai_turns(&rig.conversation())[1].id();

    Driver::result(&node).await;

    let conversation = rig.conversation();
    let turns = ai_turns(&conversation);
    assert!(interrupted(&closure(turns[0])));
    assert!(closure(turns[1]).is_none());
    assert!(!closed_turn_ids(&rig.record).contains(&other));
}

#[tokio::test(start_paused = true)]
async fn given_two_open_turns_of_the_agent_when_the_instance_starts_then_every_one_is_closed_interrupted()
 {
    let rig = seeded(|rig, conversation| {
        seed_turn(
            conversation,
            rig.agent,
            text_part("one"),
            ResponseEnding::provider_continuation(),
            None,
        );
        seed_turn(
            conversation,
            rig.agent,
            text_part("two"),
            ResponseEnding::provider_continuation(),
            None,
        );
    });
    let model = Model::new(vec![Act::text("fresh")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    let conversation = rig.conversation();
    let turns = ai_turns(&conversation);
    assert_eq!(turns.len(), 3);
    assert!(interrupted(&closure(turns[0])) && interrupted(&closure(turns[1])));
    assert_eq!(closures_given(&rig.record).len(), 3);
}
