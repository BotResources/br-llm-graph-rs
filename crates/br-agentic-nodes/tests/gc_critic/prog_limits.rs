use crate::pair::{Pair, turns_of};
use crate::ports::{Checker, Verdict, refuse};
use crate::prelude::*;

#[tokio::test(start_paused = true)]
async fn given_a_limit_of_one_when_the_first_refusal_comes_then_the_instance_fails_with_the_reason_after_storing_it()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![
        refuse("the text is 3 characters too long"),
        Verdict::Accept,
    ]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 1)
        .build();

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("programmatic critic"), "{cause}");
    assert!(cause.contains("refus"), "{cause}");
    assert!(
        cause.contains("the text is 3 characters too long"),
        "{cause}"
    );
    assert_eq!(generator.calls(), 1);
    assert_eq!(checker.checked().len(), 1);
    assert_eq!(critic.calls(), 0);
    assert_eq!(rig.outcome.calls(), 0);
    let turns = turns_of(&rig, rig.programmatic);
    assert_eq!(turns.len(), 1);
    assert_eq!(
        responses(&turns[0]),
        vec!["the text is 3 characters too long".to_owned()]
    );
    assert!(matches!(
        closure(&turns[0]),
        Some(ClosureReason::Completed { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn given_a_limit_of_three_when_three_refusals_come_in_a_row_then_the_generator_repairs_twice_and_the_third_refusal_fails()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![
        Reply::text("one"),
        Reply::text("two"),
        Reply::text("three"),
        Reply::text("four"),
    ]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![
        refuse("reason 1"),
        refuse("reason 2"),
        refuse("reason 3"),
        Verdict::Accept,
    ]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 3)
        .build();

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("programmatic critic"), "{cause}");
    assert!(cause.contains("reason 3"), "{cause}");
    assert_eq!(generator.calls(), 3);
    assert_eq!(checker.refusals_seen(), vec![0, 1, 2]);
    assert_eq!(critic.calls(), 0);
    assert_eq!(turns_of(&rig, rig.programmatic).len(), 3);
}

#[tokio::test(start_paused = true)]
async fn given_a_limit_of_three_when_two_refusals_are_repaired_then_the_instance_goes_on() {
    let rig = Rig::new();
    let generator = Scripted::new(vec![
        Reply::text("one"),
        Reply::text("two"),
        Reply::text("three"),
    ]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![
        refuse("reason 1"),
        refuse("reason 2"),
        Verdict::Accept,
    ]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 3)
        .build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!(update.text, "three");
    assert_eq!(update.iterations, Some(3));
    assert_eq!(critic.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_refusal_when_no_step_remains_then_the_instance_fails_with_the_reason_though_the_limit_is_not_reached()
 {
    let rig = Rig::new().with_step_limit(1);
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![refuse("fix the length"), Verdict::Accept]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 5)
        .build();

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("programmatic critic"), "{cause}");
    assert!(cause.contains("fix the length"), "{cause}");
    assert_eq!(generator.calls(), 1);
    assert_eq!(critic.calls(), 0);
    assert_eq!(turns_of(&rig, rig.programmatic).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_refusal_when_one_step_remains_then_the_generator_repairs() {
    let rig = Rig::new().with_step_limit(2);
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![refuse("fix the length"), Verdict::Accept]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 5)
        .build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!(update.text, "two");
}

#[tokio::test(start_paused = true)]
async fn given_a_refusal_after_the_last_pass_of_the_round_limit_ending_when_it_is_repaired_then_another_last_pass_runs_without_the_critic()
 {
    let rig = Rig::new();
    let g1 = Scripted::new(vec![Reply::text("one")]);
    let g2 = Scripted::new(vec![Reply::text("two"), Reply::text("three")]);
    let critic = Scripted::new(vec![Reply::answer(false, "improve")]);
    let checker = Checker::new(vec![
        Verdict::Accept,
        refuse("fix the length"),
        Verdict::Accept,
    ]);
    let node = Pair::ladders(&rig, ladder(vec![list(&[&g1]), list(&[&g2])]), one(&critic))
        .round_limit(1)
        .round_ending(RoundLimitEnding::FinalGeneration)
        .programmatic(&checker, 3)
        .build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::NotJudged));
    assert_eq!(update.text, "three");
    assert_eq!(update.iterations, Some(3));
    assert_eq!((g1.calls(), g2.calls()), (1, 2));
    assert_eq!(critic.calls(), 1);
    assert_eq!(
        checker.texts_seen(),
        vec!["one".to_owned(), "two".to_owned(), "three".to_owned()]
    );
    assert_eq!(checker.refusals_seen(), vec![0, 0, 1]);
}
