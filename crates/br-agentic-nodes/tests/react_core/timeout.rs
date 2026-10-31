use std::time::Duration;

use tokio::time::Instant;

use crate::common::*;
use crate::kit::*;

const SECOND: Duration = Duration::from_secs(1);

#[tokio::test(start_paused = true)]
async fn given_a_client_that_hangs_when_the_timeout_passes_then_the_next_client_answers_after_the_timeout()
 {
    let rig = Rig::new();
    let first = Model::new(vec![Act::hang()]);
    let second = Model::new(vec![Act::text("quick")]);
    let node = node(&rig, plains(&[&first, &second]));
    let started = Instant::now();

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "quick");
    assert!(started.elapsed() >= Duration::from_secs(5));
    assert!(started.elapsed() < Duration::from_secs(6));
    assert_eq!(second.calls(), 1);
    assert_eq!(
        discarded_responses(&rig.record),
        vec![first.requests()[0].reply_entry()]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_client_that_hangs_when_it_is_alone_then_the_timeout_is_a_fault() {
    let rig = Rig::new();
    let only = Model::new(vec![Act::hang()]);
    let node = node(&rig, plains(&[&only]));

    let failure = Driver::failure(&node).await;

    assert!(failure.contains("timeout"), "{failure}");
    assert!(failure.contains("5s"), "{failure}");
}

#[tokio::test(start_paused = true)]
async fn given_a_timeout_of_two_seconds_when_a_call_takes_three_then_the_call_times_out() {
    let rig = Rig::new().with_timeouts(Duration::from_secs(2), Duration::from_secs(5));
    let only = Model::new(vec![Act::text("late").after(3 * SECOND)]);
    let node = node(&rig, plains(&[&only]));

    let failure = Driver::failure(&node).await;

    assert!(failure.contains("2s"), "{failure}");
}

#[tokio::test(start_paused = true)]
async fn given_several_steps_when_each_call_is_under_the_timeout_then_the_timeouts_do_not_add_up() {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]);
    let only = Model::new(vec![
        Act::call("alpha").after(3 * SECOND),
        Act::call("alpha").after(3 * SECOND),
        Act::text("done").after(3 * SECOND),
    ]);
    let node = node(&rig, plains(&[&only]));

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    assert_eq!(update.steps, Some(3));
}

#[tokio::test(start_paused = true)]
async fn given_a_fragment_that_the_stream_holds_when_the_client_sends_it_then_the_call_times_out() {
    let rig = Rig::new();
    let only = Model::new(vec![Act::text("hello").streaming()]);
    let quirks = Quirks {
        fragment: Fragment::Hold,
        ..Quirks::default()
    };
    let node = node_over(&rig, Wrapped::new(&rig.access, quirks), plains(&[&only]));

    let failure = Driver::failure(&node).await;

    assert!(failure.contains("timeout"), "{failure}");
}

#[tokio::test(start_paused = true)]
async fn given_a_fragment_that_the_stream_holds_when_the_list_has_a_next_client_then_the_call_switches()
 {
    let rig = Rig::new();
    let first = Model::new(vec![Act::text("held").streaming()]);
    let second = Model::new(vec![Act::text("fine")]);
    let quirks = Quirks {
        fragment: Fragment::Hold,
        ..Quirks::default()
    };
    let node = node_over(
        &rig,
        Wrapped::new(&rig.access, quirks),
        plains(&[&first, &second]),
    );

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "fine");
    assert_eq!(
        discarded_responses(&rig.record),
        vec![first.requests()[0].reply_entry()]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_call_and_a_fragment_hand_over_when_together_they_pass_the_timeout_then_the_call_times_out()
 {
    let rig = Rig::new();
    let only = Model::new(vec![Act::text("slow").after(3 * SECOND).streaming()]);
    let quirks = Quirks {
        fragment: Fragment::Delay(3 * SECOND),
        ..Quirks::default()
    };
    let node = node_over(&rig, Wrapped::new(&rig.access, quirks), plains(&[&only]));

    let failure = Driver::failure(&node).await;

    assert!(failure.contains("timeout"), "{failure}");
}

#[tokio::test(start_paused = true)]
async fn given_a_call_and_a_fragment_hand_over_when_together_they_stay_under_the_timeout_then_the_call_works()
 {
    let rig = Rig::new();
    let only = Model::new(vec![Act::text("slow").after(3 * SECOND).streaming()]);
    let quirks = Quirks {
        fragment: Fragment::Delay(SECOND),
        ..Quirks::default()
    };
    let node = node_over(&rig, Wrapped::new(&rig.access, quirks), plains(&[&only]));

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "slow");
}
