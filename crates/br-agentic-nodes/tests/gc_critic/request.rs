use crate::pair::Pair;
use crate::ports::{Requested, Requester};
use crate::prelude::*;
use crate::seen::last_human;

fn known_request(text: &str) -> CriticRequest {
    CriticRequest {
        text: text.to_owned(),
        human: human_id(),
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_fixed_request_when_the_critic_is_called_then_it_is_the_last_entry_of_the_projection_with_the_given_identity()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let request = known_request("judge this please");
    let human = request.human;
    let node = Pair::new(&rig, &generator, &critic)
        .request_from(request)
        .build();

    Driver::result(&node).await;

    let (_, who, text) = last_human(&critic.requests()[0]);
    assert_eq!(text, "judge this please");
    assert_eq!(who, human);
}

#[tokio::test(start_paused = true)]
async fn given_a_fixed_request_when_the_critic_is_called_then_it_comes_after_the_work_of_the_generator()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("the generated draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::new(&rig, &generator, &critic)
        .request("judge this please")
        .build();

    Driver::result(&node).await;

    let seen = crate::seen::seen_by(&critic.requests()[0]);
    let flat: Vec<_> = seen.iter().flat_map(|turn| turn.entries.clone()).collect();
    let position = |wanted: &str| {
        flat.iter().position(|entry| match entry {
            crate::seen::Seen::Output { text, .. } | crate::seen::Seen::Human { text, .. } => {
                text == wanted
            }
            _ => false,
        })
    };
    let draft = position("the generated draft").unwrap();
    let request = position("judge this please").unwrap();
    assert_eq!(request, flat.len() - 1);
    assert!(draft < request);
}

#[tokio::test(start_paused = true)]
async fn given_a_request_when_the_instance_runs_then_it_is_never_stored_nor_shown_to_the_generator()
{
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "improve"),
        Reply::answer(true, ""),
    ]);
    let node = Pair::new(&rig, &generator, &critic)
        .request("REQUEST-TEXT-MARKER")
        .build();

    Driver::result(&node).await;

    assert!(!format!("{:?}", rig.conversation()).contains("REQUEST-TEXT-MARKER"));
    assert!(!format!("{:?}", rig.record.calls()).contains("REQUEST-TEXT-MARKER"));
    for request in generator.requests() {
        assert!(!history_text(&request).contains("REQUEST-TEXT-MARKER"));
    }
    assert!(history_text(&critic.requests()[1]).contains("REQUEST-TEXT-MARKER"));
}

#[tokio::test(start_paused = true)]
async fn given_a_fixed_request_when_the_critic_is_called_twice_then_each_call_has_a_new_entry_identity_and_the_same_text()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "improve"),
        Reply::answer(true, ""),
    ]);
    let node = Pair::new(&rig, &generator, &critic)
        .request("judge this please")
        .build();

    Driver::result(&node).await;

    let requests = critic.requests();
    let (first, first_human, first_text) = last_human(&requests[0]);
    let (second, second_human, second_text) = last_human(&requests[1]);
    assert_ne!(first, second);
    assert_eq!(first_text, second_text);
    assert_eq!(first_human, second_human);
}

#[tokio::test(start_paused = true)]
async fn given_a_fallback_list_when_the_first_attempt_fails_then_the_next_attempt_has_a_new_entry_identity()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let first = Scripted::new(vec![Reply::text("not structured")]);
    let second = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::ladders(
        &rig,
        one(&generator),
        ladder(vec![list(&[&first, &second])]),
    )
    .request("judge this please")
    .build();

    Driver::result(&node).await;

    let (a, _, a_text) = last_human(&first.requests()[0]);
    let (b, _, b_text) = last_human(&second.requests()[0]);
    assert_ne!(a, b);
    assert_eq!(a_text, b_text);
}

#[tokio::test(start_paused = true)]
async fn given_a_request_port_when_a_round_runs_then_it_is_called_with_the_last_response_of_the_generator_and_gives_the_request()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("the draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let port = Requester::new(Requested::Text("port request"));
    let node = Pair::new(&rig, &generator, &critic)
        .request_port(&port)
        .build();

    Driver::result(&node).await;

    assert_eq!(port.calls(), vec!["the draft".to_owned()]);
    let (_, who, text) = last_human(&critic.requests()[0]);
    assert_eq!(text, "port request #1");
    assert_eq!(who, port.human());
}

#[tokio::test(start_paused = true)]
async fn given_a_request_port_when_the_attempts_of_a_round_run_on_a_fallback_list_then_the_port_is_called_once_and_its_result_is_reused()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("the draft")]);
    let first = Scripted::new(vec![Reply::text("not structured")]);
    let second = Scripted::new(vec![Reply::answer(false, "")]);
    let third = Scripted::new(vec![Reply::answer(true, "")]);
    let port = Requester::new(Requested::Text("port request"));
    let node = Pair::ladders(
        &rig,
        one(&generator),
        ladder(vec![list(&[&first, &second, &third])]),
    )
    .request_port(&port)
    .build();

    Driver::result(&node).await;

    assert_eq!(port.calls().len(), 1);
    for attempt in [&first, &second, &third] {
        let (_, who, text) = last_human(&attempt.requests()[0]);
        assert_eq!(text, "port request #1");
        assert_eq!(who, port.human());
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_request_port_when_two_rounds_run_then_it_is_called_once_per_round_with_that_round_response()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft one"), Reply::text("draft two")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "improve"),
        Reply::answer(true, ""),
    ]);
    let port = Requester::new(Requested::Text("port request"));
    let node = Pair::new(&rig, &generator, &critic)
        .request_port(&port)
        .build();

    Driver::result(&node).await;

    assert_eq!(
        port.calls(),
        vec!["draft one".to_owned(), "draft two".to_owned()]
    );
    let requests = critic.requests();
    assert_eq!(last_human(&requests[0]).2, "port request #1");
    assert_eq!(last_human(&requests[1]).2, "port request #2");
}

#[tokio::test(start_paused = true)]
async fn given_a_blank_fixed_request_when_the_critic_would_be_called_then_the_instance_fails_without_calling_it()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::new(&rig, &generator, &critic)
        .request("  \n ")
        .build();

    let cause = Driver::failure(&node).await;

    assert!(!cause.is_empty());
    assert_eq!(critic.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_request_port_that_gives_a_blank_text_when_the_critic_would_be_called_then_the_instance_fails_without_calling_it()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let port = Requester::new(Requested::Blank);
    let node = Pair::new(&rig, &generator, &critic)
        .request_port(&port)
        .build();

    let cause = Driver::failure(&node).await;

    assert!(!cause.is_empty());
    assert_eq!(critic.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_request_port_that_fails_when_the_critic_would_be_called_then_the_instance_fails_without_calling_it()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let port = Requester::new(Requested::Fail);
    let node = Pair::new(&rig, &generator, &critic)
        .request_port(&port)
        .build();

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("critic-request port failed"), "{cause}");
    assert_eq!(critic.calls(), 0);
}
