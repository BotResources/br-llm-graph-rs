use crate::pair::Pair;
use crate::ports::{Checker, Verdict, refuse};
use crate::prelude::*;

#[tokio::test(start_paused = true)]
async fn given_refusals_in_a_row_when_a_request_item_is_stored_then_the_programmatic_refusal_counter_goes_back_to_zero()
 {
    let rig = Rig::new().with_inbox();
    let inbox = rig.inbox.clone();
    let agent = rig.generator;
    let generator = Scripted::new(vec![
        Reply::text("one"),
        Reply::text("two"),
        Reply::text("three"),
        Reply::text("four"),
        Reply::text("five"),
    ]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::hooked(
        vec![
            refuse("reason 1"),
            refuse("reason 2"),
            refuse("reason 3"),
            Verdict::Accept,
        ],
        move |call| {
            if call == 1 {
                inbox.deliver_entry(agent, inbox_entry("a late request"));
            }
        },
    );
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 3)
        .build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    let seen = checker.refusals_seen();
    assert_eq!(seen.len(), 4, "{seen:?}");
    assert_eq!(seen[0], 0);
    assert!(seen.iter().skip(1).any(|count| *count == 0), "{seen:?}");
}

#[tokio::test(start_paused = true)]
async fn given_refusals_in_a_row_when_no_request_item_is_stored_then_the_counter_keeps_growing_until_the_limit()
 {
    let rig = Rig::new().with_inbox();
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

    assert!(cause.contains("reason 3"), "{cause}");
    assert_eq!(checker.refusals_seen(), vec![0, 1, 2]);
}
