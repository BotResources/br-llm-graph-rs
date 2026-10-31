use std::time::Duration;

use crate::pair::{Pair, turns_of};
use crate::prelude::*;

#[tokio::test(start_paused = true)]
async fn given_a_critic_call_that_exceeds_the_timeout_when_the_list_has_another_client_then_the_next_client_is_called()
 {
    let rig = Rig::new().with_timeouts(Duration::from_secs(2), Duration::from_secs(5));
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let slow = Scripted::new(vec![Reply::Hang]);
    let fast = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::ladders(&rig, one(&generator), ladder(vec![list(&[&slow, &fast])])).build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!((slow.calls(), fast.calls()), (1, 1));
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_call_that_exceeds_the_timeout_when_the_list_has_no_other_client_then_the_instance_fails()
 {
    let rig = Rig::new().with_timeouts(Duration::from_secs(2), Duration::from_secs(5));
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let slow = Scripted::new(vec![Reply::Hang]);
    let node = Pair::new(&rig, &generator, &slow).build();

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("timeout"), "{cause}");
    assert_eq!(slow.calls(), 1);
    assert!(turns_of(&rig, rig.critic).is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_list_of_clients_that_hang_when_the_critic_is_called_then_each_call_has_its_own_timeout()
 {
    let rig = Rig::new().with_timeouts(Duration::from_secs(2), Duration::from_secs(5));
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let first = Scripted::new(vec![Reply::Hang]);
    let second = Scripted::new(vec![Reply::Hang]);
    let third = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::ladders(
        &rig,
        one(&generator),
        ladder(vec![list(&[&first, &second, &third])]),
    )
    .build();
    let started = tokio::time::Instant::now();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    let waited = started.elapsed();
    assert!(waited >= Duration::from_secs(4), "{waited:?}");
    assert!(waited < Duration::from_secs(6), "{waited:?}");
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_call_that_answers_before_the_timeout_when_it_is_awaited_then_no_switch_happens()
 {
    let rig = Rig::new().with_timeouts(Duration::from_secs(2), Duration::from_secs(5));
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let first = Scripted::new(vec![Reply::answer(true, "")]);
    let second = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::ladders(
        &rig,
        one(&generator),
        ladder(vec![list(&[&first, &second])]),
    )
    .build();

    Driver::result(&node).await;

    assert_eq!((first.calls(), second.calls()), (1, 0));
}
