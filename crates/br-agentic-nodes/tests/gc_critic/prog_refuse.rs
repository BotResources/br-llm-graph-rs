use br_agentic_contract::memory::Call;
use br_llm_messages::ContentBlock;

use crate::pair::{Pair, calls_for, turns_of};
use crate::ports::{Checker, Verdict, log, mark_on, marks, refuse};
use crate::prelude::*;
use crate::seen::{Seen, seen_by};

const REASON: &str = "The generated text has 53 characters, 3 above the limit of 50";

#[tokio::test(start_paused = true)]
async fn given_a_refusal_when_the_reason_is_stored_then_it_is_an_ai_turn_of_the_programmatic_critic_with_its_author()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![refuse(REASON), Verdict::Accept]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 3)
        .build();

    Driver::result(&node).await;

    let turns = turns_of(&rig, rig.programmatic);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].agent(), rig.programmatic);
    assert_eq!(turns[0].author().as_str(), "Checker");
    assert_eq!(responses(&turns[0]), vec![REASON.to_owned()]);
}

#[tokio::test(start_paused = true)]
async fn given_a_refusal_when_the_reason_is_stored_then_the_response_has_one_text_block_ends_finished_and_the_turn_closes_completed()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![
        refuse("  spaced   reason\nline two "),
        Verdict::Accept,
    ]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 3)
        .build();

    Driver::result(&node).await;

    let turns = turns_of(&rig, rig.programmatic);
    let response = turns[0]
        .entries()
        .iter()
        .find_map(|entry| match entry {
            br_llm_messages::TurnEntry::ModelResponse(response) => Some(response.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(response.blocks().len(), 1);
    assert!(matches!(response.blocks()[0], ContentBlock::Text(_)));
    assert_eq!(text_of(&response), "  spaced   reason\nline two ");
    assert!(matches!(response.ending(), ResponseEnding::Finished { .. }));
    assert!(matches!(
        closure(&turns[0]),
        Some(ClosureReason::Completed { .. })
    ));
    assert_eq!(
        calls_for(&rig, rig.programmatic),
        vec!["complete_response", "close_turn"]
    );
    assert!(!rig.record.calls().iter().any(|call| matches!(
        call,
        Call::OpenResponse { turn, .. } if turn.agent == rig.programmatic
    )));
}

#[tokio::test(start_paused = true)]
async fn given_a_refusal_when_the_round_runs_then_the_critic_is_not_called_until_the_programmatic_critic_accepts()
 {
    let rig = Rig::new();
    let marks_log = log();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    generator.on_call(mark_on(&marks_log, "generator"));
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    critic.on_call(mark_on(&marks_log, "critic"));
    let checker = Checker::logging(vec![refuse(REASON), Verdict::Accept], &marks_log);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 3)
        .build();

    Driver::result(&node).await;

    assert_eq!(
        marks(&marks_log),
        vec![
            "generator",
            "programmatic",
            "generator",
            "programmatic",
            "critic"
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_refusal_when_the_generator_repairs_then_it_sees_the_reason_as_output_of_another_agent()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![refuse(REASON), Verdict::Accept]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 3)
        .build();

    Driver::result(&node).await;

    let seen = seen_by(&generator.requests()[1]);
    let outputs: Vec<_> = seen
        .iter()
        .flat_map(|turn| turn.entries.iter())
        .filter_map(|entry| match entry {
            Seen::Output {
                agent,
                author,
                text,
            } if *agent == rig.programmatic => Some((author.clone(), text.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(outputs, vec![("Checker".to_owned(), REASON.to_owned())]);
}

#[tokio::test(start_paused = true)]
async fn given_a_refusal_before_the_critic_is_called_when_the_critic_reads_the_thread_then_it_sees_the_reason_as_output_of_another_agent()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![refuse(REASON), Verdict::Accept]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 3)
        .build();

    Driver::result(&node).await;

    let seen = seen_by(&critic.requests()[0]);
    assert!(seen.iter().all(|turn| turn.ai_agent.is_none()));
    let outputs: Vec<_> = seen
        .iter()
        .flat_map(|turn| turn.entries.iter())
        .filter_map(|entry| match entry {
            Seen::Output { agent, text, .. } => Some((*agent, text.clone())),
            _ => None,
        })
        .collect();
    assert!(outputs.contains(&(rig.programmatic, REASON.to_owned())));
    assert!(outputs.contains(&(rig.generator, "two".to_owned())));
}

#[tokio::test(start_paused = true)]
async fn given_refusals_when_passes_repair_then_the_counter_is_given_to_each_call_and_goes_back_to_zero_on_accept()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![
        Reply::text("one"),
        Reply::text("two"),
        Reply::text("three"),
        Reply::text("four"),
    ]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "improve"),
        Reply::answer(true, ""),
    ]);
    let checker = Checker::new(vec![
        refuse("first"),
        Verdict::Accept,
        refuse("second"),
        Verdict::Accept,
    ]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 2)
        .build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!(checker.refusals_seen(), vec![0, 1, 0, 1]);
}

#[tokio::test(start_paused = true)]
async fn given_a_refusal_when_the_pass_is_repaired_then_the_round_counter_and_the_ladder_position_do_not_change()
 {
    let rig = Rig::new();
    let g1 = Scripted::new(vec![Reply::text("a")]);
    let g2 = Scripted::new(vec![Reply::text("b"), Reply::text("c")]);
    let g3 = Scripted::new(vec![Reply::text("never")]);
    let c1 = Scripted::new(vec![Reply::answer(false, "improve")]);
    let c2 = Scripted::new(vec![Reply::answer(true, "")]);
    let c3 = Scripted::new(vec![Reply::answer(true, "wrong entry")]);
    let checker = Checker::new(vec![Verdict::Accept, refuse(REASON), Verdict::Accept]);
    let node = Pair::ladders(
        &rig,
        ladder(vec![list(&[&g1]), list(&[&g2]), list(&[&g3])]),
        ladder(vec![list(&[&c1]), list(&[&c2]), list(&[&c3])]),
    )
    .programmatic(&checker, 3)
    .build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!((g1.calls(), g2.calls(), g3.calls()), (1, 2, 0));
    assert_eq!((c1.calls(), c2.calls(), c3.calls()), (1, 1, 0));
    assert_eq!(update.iterations, Some(3));
}

#[tokio::test(start_paused = true)]
async fn given_a_blank_explicit_reason_when_the_programmatic_critic_refuses_then_the_instance_fails()
 {
    for blank in ["", "   ", " \n\t "] {
        let rig = Rig::new();
        let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
        let critic = Scripted::new(vec![Reply::answer(true, "")]);
        let checker = Checker::new(vec![refuse(blank), Verdict::Accept]);
        let node = Pair::new(&rig, &generator, &critic)
            .programmatic(&checker, 3)
            .build();

        let cause = Driver::failure(&node).await;

        assert!(!cause.is_empty());
        assert_eq!(checker.checked().len(), 1, "{blank:?}");
        assert_eq!(generator.calls(), 1);
        assert_eq!(critic.calls(), 0);
        assert!(turns_of(&rig, rig.programmatic).is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn given_an_error_of_the_programmatic_critic_port_when_it_is_called_then_the_instance_fails()
{
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![Verdict::Fail]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 3)
        .build();

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("programmatic critic port failed"), "{cause}");
    assert_eq!(critic.calls(), 0);
    assert!(turns_of(&rig, rig.programmatic).is_empty());
}
