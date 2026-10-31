use br_agentic_nodes::Judgement;

use crate::common::*;
use crate::items::*;
use crate::kit::*;
use crate::view::*;

#[tokio::test(start_paused = true)]
async fn given_a_validation_with_a_waiting_notice_when_the_new_pass_starts_then_the_counter_is_not_reset()
 {
    let rig = Rig::new().with_inbox();
    let (_, deliver) = notice_item(&rig, "heads up");
    let first = Scripted::new(vec![Reply::text("d1")]);
    let second = Scripted::new(vec![Reply::text("d2"), Reply::text("d3")]);
    deliver_on(&second, 1, deliver);
    let c1 = Scripted::new(vec![Reply::answer(false, "m1")]);
    let c2 = Scripted::new(vec![Reply::answer(true, ""), Reply::answer(true, "")]);
    let node = build(
        &rig,
        mixed(&[(&first, false), (&second, true)]),
        rungs(&[&c1, &c2]),
        5,
        Setup::default(),
    );

    let update = outcome_of(&node).await;

    assert_eq!(calls_of(&[&first, &second]), vec![1, 2]);
    assert_eq!(calls_of(&[&c1, &c2]), vec![1, 2]);
    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!(update.iterations, Some(3));
    assert!(timeline(&rig).contains(&"notice:heads up".to_owned()));
}
