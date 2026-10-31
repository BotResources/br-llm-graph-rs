use br_agentic_contract::memory::Call;
use br_graph_contract::Event;
use br_llm_messages::{AITurn, EntryId, TurnEntry};

use crate::pair::{Pair, calls_for, turns_of};
use crate::prelude::*;

fn response_entries(turn: &AITurn) -> Vec<EntryId> {
    turn.entries()
        .iter()
        .filter_map(|entry| match entry {
            TurnEntry::ModelResponse(response) => Some(response.id()),
            _ => None,
        })
        .collect()
}

fn the_only_response(turn: &AITurn) -> &br_llm_messages::ModelResponse {
    let found: Vec<_> = turn
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            TurnEntry::ModelResponse(response) => Some(response),
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1);
    found[0]
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_that_validates_when_the_instance_ends_then_nothing_of_the_call_is_stored() {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "kept out of the thread")]);
    let node = Pair::new(&rig, &generator, &critic).build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert!(turns_of(&rig, rig.critic).is_empty());
    assert!(calls_for(&rig, rig.critic).is_empty());
    assert!(!format!("{:?}", rig.conversation()).contains("kept out of the thread"));
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_that_does_not_validate_when_the_message_is_stored_then_it_is_a_new_ai_turn_of_the_critic_with_its_author()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "make it  shorter\n  and clearer "),
        Reply::answer(true, ""),
    ]);
    let node = Pair::new(&rig, &generator, &critic).build();

    Driver::result(&node).await;

    let turns = turns_of(&rig, rig.critic);
    assert_eq!(turns.len(), 1);
    let turn = &turns[0];
    assert_eq!(turn.agent(), rig.critic);
    assert_eq!(turn.author().as_str(), "Critic");
    assert_ne!(turn.agent(), rig.generator);
    assert_eq!(
        responses(turn),
        vec!["make it  shorter\n  and clearer ".to_owned()]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_stored_message_when_the_turn_is_read_then_it_holds_one_response_with_one_text_block_ending_finished()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "improve"),
        Reply::answer(true, ""),
    ]);
    let node = Pair::new(&rig, &generator, &critic).build();

    Driver::result(&node).await;

    let turns = turns_of(&rig, rig.critic);
    let response = the_only_response(&turns[0]);
    assert_eq!(response.blocks().len(), 1);
    assert!(matches!(
        response.blocks()[0],
        br_llm_messages::ContentBlock::Text(_)
    ));
    assert!(matches!(response.ending(), ResponseEnding::Finished { .. }));
}

#[tokio::test(start_paused = true)]
async fn given_a_stored_message_when_the_turn_ends_then_it_is_closed_completed_after_the_response()
{
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "improve"),
        Reply::answer(true, ""),
    ]);
    let node = Pair::new(&rig, &generator, &critic).build();

    Driver::result(&node).await;

    let turns = turns_of(&rig, rig.critic);
    assert!(matches!(
        closure(&turns[0]),
        Some(ClosureReason::Completed { .. })
    ));
    assert_eq!(
        calls_for(&rig, rig.critic),
        vec!["complete_response", "close_turn"]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_stored_message_when_the_response_is_completed_then_it_was_never_opened_and_no_stream_was_used()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "improve"),
        Reply::answer(true, ""),
    ]);
    let node = Pair::new(&rig, &generator, &critic).build();

    Driver::result(&node).await;

    let turns = turns_of(&rig, rig.critic);
    let entries = response_entries(&turns[0]);
    assert_eq!(entries.len(), 1);
    let calls = rig.record.calls();
    assert!(!calls.iter().any(|call| matches!(
        call,
        Call::OpenResponse { entry, .. } | Call::ResponseDelta { entry, .. } | Call::DiscardResponse { entry, .. }
            if *entry == entries[0]
    )));
    assert!(!calls_for(&rig, rig.critic).contains(&"open_response"));
    let opened = calls
        .iter()
        .filter(|call| matches!(call, Call::OpenResponse { .. }))
        .count();
    assert_eq!(opened, generator.calls());
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_that_does_not_validate_twice_when_the_messages_are_stored_then_each_is_a_new_turn()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![
        Reply::text("one"),
        Reply::text("two"),
        Reply::text("three"),
    ]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "first remark"),
        Reply::answer(false, "second remark"),
        Reply::answer(true, ""),
    ]);
    let node = Pair::new(&rig, &generator, &critic).build();

    Driver::result(&node).await;

    let turns = turns_of(&rig, rig.critic);
    assert_eq!(turns.len(), 2);
    assert_ne!(turns[0].id(), turns[1].id());
    assert_eq!(responses(&turns[0]), vec!["first remark".to_owned()]);
    assert_eq!(responses(&turns[1]), vec!["second remark".to_owned()]);
    for turn in &turns {
        assert!(matches!(
            closure(turn),
            Some(ClosureReason::Completed { .. })
        ));
    }
}

#[tokio::test(start_paused = true)]
async fn given_the_round_limit_with_final_generation_when_the_critic_does_not_validate_then_the_message_is_stored()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![Reply::answer(false, "last remark")]);
    let node = Pair::new(&rig, &generator, &critic)
        .round_limit(1)
        .round_ending(RoundLimitEnding::FinalGeneration)
        .build();

    Driver::result(&node).await;

    let turns = turns_of(&rig, rig.critic);
    assert_eq!(turns.len(), 1);
    assert_eq!(responses(&turns[0]), vec!["last remark".to_owned()]);
}

#[tokio::test(start_paused = true)]
async fn given_the_round_limit_with_keep_when_the_critic_does_not_validate_then_the_message_is_stored()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one")]);
    let critic = Scripted::new(vec![Reply::answer(false, "last remark")]);
    let node = Pair::new(&rig, &generator, &critic)
        .round_limit(1)
        .round_ending(RoundLimitEnding::KeepRejectedGeneration)
        .build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Rejected));
    let turns = turns_of(&rig, rig.critic);
    assert_eq!(turns.len(), 1);
    assert_eq!(responses(&turns[0]), vec!["last remark".to_owned()]);
}

#[tokio::test(start_paused = true)]
async fn given_the_round_limit_with_fail_when_the_critic_does_not_validate_then_nothing_of_the_call_is_stored()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one")]);
    let critic = Scripted::new(vec![Reply::answer(false, "last remark")]);
    let node = Pair::new(&rig, &generator, &critic)
        .round_limit(1)
        .round_ending(RoundLimitEnding::Fail)
        .build();

    let (_driver, event) = Driver::run(&node).await;

    assert!(matches!(event, Event::Failed(_)));
    assert!(turns_of(&rig, rig.critic).is_empty());
    assert!(!format!("{:?}", rig.conversation()).contains("last remark"));
}

#[tokio::test(start_paused = true)]
async fn given_a_stored_message_when_the_generator_runs_again_then_it_sees_the_message_as_output_of_another_agent()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "improve"),
        Reply::answer(true, ""),
    ]);
    let node = Pair::new(&rig, &generator, &critic).build();

    Driver::result(&node).await;

    let second = &generator.requests()[1];
    let seen = crate::seen::seen_by(second);
    let outputs: Vec<_> = seen
        .iter()
        .flat_map(|turn| turn.entries.iter())
        .filter_map(|entry| match entry {
            crate::seen::Seen::Output {
                agent,
                author,
                text,
            } if *agent == rig.critic => Some((author.clone(), text.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(outputs, vec![("Critic".to_owned(), "improve".to_owned())]);
    assert!(seen.iter().all(|turn| turn.ai_agent != Some(rig.critic)));
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_client_that_streams_when_the_critic_is_called_then_no_fragment_reaches_the_thread_access()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = crate::raw::RawClient::streaming(vec![
        crate::raw::RawReply::answer(false, "improve", ResponseEnding::finished()),
        crate::raw::RawReply::answer(true, "", ResponseEnding::finished()),
    ]);
    let entry = ladder(vec![FallbackList::from(ModelClient::new(
        critic.clone(),
        ThinkingExpectation::NotExpected,
    ))]);
    let node = Pair::ladders(&rig, one(&generator), entry).build();

    Driver::result(&node).await;

    assert_eq!(critic.calls(), 2);
    let calls = rig.record.calls();
    assert!(
        !calls
            .iter()
            .any(|call| matches!(call, Call::ResponseDelta { .. }))
    );
    let opened = calls
        .iter()
        .filter(|call| matches!(call, Call::OpenResponse { .. }))
        .count();
    assert_eq!(opened, generator.calls());
}
