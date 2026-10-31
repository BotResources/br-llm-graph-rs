use std::sync::Arc;

use br_llm_clients_core::ClientError;

use crate::pair::Pair;
use crate::prelude::*;
use crate::seen::shape_names;

fn expected_ladder(critic: &Arc<Scripted>) -> Ladder {
    ladder(vec![FallbackList::from(thinking_client(critic))])
}

#[tokio::test(start_paused = true)]
async fn given_a_client_not_expected_to_think_when_the_critic_is_called_then_the_shape_has_thinking_before_the_two_keys()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::new(&rig, &generator, &critic).build();

    Driver::result(&node).await;

    let requests = critic.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        shape_names(&requests[0]),
        vec!["thinking", "is_validated", "message"]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_client_expected_to_think_when_the_critic_is_called_then_the_shape_has_the_two_keys_only()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::thinking(Reply::answer(true, ""))]);
    let node = Pair::ladders(&rig, one(&generator), expected_ladder(&critic)).build();

    Driver::result(&node).await;

    assert_eq!(
        shape_names(&critic.requests()[0]),
        vec!["is_validated", "message"]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_list_whose_first_client_is_expected_to_think_when_it_fails_then_each_attempt_has_the_shape_of_its_own_client()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let first = Scripted::new(vec![Reply::Fail(ClientError::Transport)]);
    let second = Scripted::new(vec![Reply::answer(true, "")]);
    let entry = FallbackList::new(vec![thinking_client(&first), client(&second)]).unwrap();
    let node = Pair::ladders(&rig, one(&generator), ladder(vec![entry])).build();

    Driver::result(&node).await;

    assert_eq!(
        shape_names(&first.requests()[0]),
        vec!["is_validated", "message"]
    );
    assert_eq!(
        shape_names(&second.requests()[0]),
        vec!["thinking", "is_validated", "message"]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_list_whose_first_client_is_not_expected_to_think_when_it_fails_then_each_attempt_has_the_shape_of_its_own_client()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let first = Scripted::new(vec![Reply::Fail(ClientError::Transport)]);
    let second = Scripted::new(vec![Reply::thinking(Reply::answer(true, ""))]);
    let entry = FallbackList::new(vec![client(&first), thinking_client(&second)]).unwrap();
    let node = Pair::ladders(&rig, one(&generator), ladder(vec![entry])).build();

    Driver::result(&node).await;

    assert_eq!(
        shape_names(&first.requests()[0]),
        vec!["thinking", "is_validated", "message"]
    );
    assert_eq!(
        shape_names(&second.requests()[0]),
        vec!["is_validated", "message"]
    );
}

#[tokio::test(start_paused = true)]
async fn given_two_rounds_when_the_critic_is_called_twice_then_the_shape_follows_the_client_each_time()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::thinking(Reply::answer(false, "again")),
        Reply::thinking(Reply::answer(true, "")),
    ]);
    let node = Pair::ladders(&rig, one(&generator), expected_ladder(&critic)).build();

    Driver::result(&node).await;

    let requests = critic.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(shape_names(request), vec!["is_validated", "message"]);
    }
}
