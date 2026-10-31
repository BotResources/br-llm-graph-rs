use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::pair::Pair;
use crate::ports::{Checker, Verdict, refuse};
use crate::prelude::*;

#[tokio::test(start_paused = true)]
async fn given_a_refused_pass_whose_drain_reset_the_round_counter_when_the_generator_repairs_then_the_position_of_the_refused_pass_is_kept()
 {
    let (slow, _probe) = tool(
        "slow",
        Behavior::Slow(Duration::from_millis(10), "slept".to_owned()),
    );
    let rig = Rig::new().with_inbox().with_tools(vec![slow]);
    let inbox = rig.inbox.clone();
    let agent = rig.generator;
    let g1 = Scripted::new(vec![Reply::text("a")]);
    let g2 = Scripted::new(vec![
        Reply::call("slow"),
        Reply::call("slow"),
        Reply::text("b"),
        Reply::text("c"),
    ]);
    let seen_calls = AtomicUsize::new(0);
    g2.on_call(move |_| {
        if seen_calls.fetch_add(1, Ordering::SeqCst) == 0 {
            inbox.deliver_entry(agent, inbox_entry("a request in the middle of the pass"));
        }
    });
    let c1 = Scripted::new(vec![Reply::answer(false, "improve")]);
    let c2 = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![
        Verdict::Accept,
        refuse("fix the draft"),
        Verdict::Accept,
    ]);
    let node = Pair::ladders(
        &rig,
        ladder(vec![list(&[&g1]), list(&[&g2])]),
        ladder(vec![list(&[&c1]), list(&[&c2])]),
    )
    .programmatic(&checker, 3)
    .build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert!(history_text(&g2.requests()[2]).contains("a request in the middle of the pass"));
    assert_eq!(g1.calls(), 1);
    assert_eq!(g2.calls(), 4);
    assert_eq!((c1.calls(), c2.calls()), (1, 1));
    assert_eq!(update.text, "c");
}
