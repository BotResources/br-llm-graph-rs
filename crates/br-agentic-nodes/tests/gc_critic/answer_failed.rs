use std::sync::Arc;

use br_llm_clients_core::ClientError;

use crate::pair::{Pair, turns_of};
use crate::prelude::*;
use crate::raw::{RawClient, RawReply};

fn list_of(first: &Arc<Scripted>, second: &Arc<Scripted>) -> Ladder {
    ladder(vec![list(&[first, second])])
}

async fn falls_back_from(bad: Reply) {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let first = Scripted::new(vec![bad]);
    let second = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::ladders(&rig, one(&generator), list_of(&first, &second)).build();

    let update = Driver::result(&node).await;

    assert_eq!(first.calls(), 1);
    assert_eq!(second.calls(), 1);
    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert!(turns_of(&rig, rig.critic).is_empty());
}

async fn faults_alone(bad: Reply) {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![bad]);
    let node = Pair::new(&rig, &generator, &critic).build();

    let cause = Driver::failure(&node).await;

    assert!(!cause.is_empty());
    assert_eq!(critic.calls(), 1);
    assert!(turns_of(&rig, rig.critic).is_empty());
}

macro_rules! failed_call {
    ($falls:ident, $faults:ident, $bad:expr) => {
        #[tokio::test(start_paused = true)]
        async fn $falls() {
            falls_back_from($bad).await;
        }

        #[tokio::test(start_paused = true)]
        async fn $faults() {
            faults_alone($bad).await;
        }
    };
}

failed_call!(
    given_a_text_answer_without_structure_when_the_list_has_another_client_then_the_next_client_is_called,
    given_a_text_answer_without_structure_when_the_list_is_used_up_then_the_instance_fails,
    Reply::text("looks fine to me")
);

failed_call!(
    given_an_answer_without_is_validated_when_the_list_has_another_client_then_the_next_client_is_called,
    given_an_answer_without_is_validated_when_the_list_is_used_up_then_the_instance_fails,
    Reply::Structured("{\"message\":\"fix it\"}".to_owned())
);

failed_call!(
    given_an_answer_with_a_wrong_type_when_the_list_has_another_client_then_the_next_client_is_called,
    given_an_answer_with_a_wrong_type_when_the_list_is_used_up_then_the_instance_fails,
    Reply::Structured("{\"is_validated\":\"yes\",\"message\":\"x\"}".to_owned())
);

failed_call!(
    given_a_rejection_without_message_when_the_list_has_another_client_then_the_next_client_is_called,
    given_a_rejection_without_message_when_the_list_is_used_up_then_the_instance_fails,
    Reply::Structured("{\"is_validated\":false}".to_owned())
);

failed_call!(
    given_a_rejection_with_an_empty_message_when_the_list_has_another_client_then_the_next_client_is_called,
    given_a_rejection_with_an_empty_message_when_the_list_is_used_up_then_the_instance_fails,
    Reply::answer(false, "")
);

failed_call!(
    given_a_rejection_with_a_blank_message_when_the_list_has_another_client_then_the_next_client_is_called,
    given_a_rejection_with_a_blank_message_when_the_list_is_used_up_then_the_instance_fails,
    Reply::answer(false, "  \n\t ")
);

failed_call!(
    given_an_error_of_the_client_when_the_list_has_another_client_then_the_next_client_is_called,
    given_an_error_of_the_client_when_the_list_is_used_up_then_the_instance_fails,
    Reply::Fail(ClientError::Transport)
);

async fn raw_falls_back_from(bad: RawReply) {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let first = RawClient::new(vec![bad]);
    let second = Scripted::new(vec![Reply::answer(true, "")]);
    let entry = FallbackList::new(vec![
        ModelClient::new(first.clone(), ThinkingExpectation::NotExpected),
        client(&second),
    ])
    .unwrap();
    let node = Pair::ladders(&rig, one(&generator), ladder(vec![entry])).build();

    let update = Driver::result(&node).await;

    assert_eq!(first.calls(), 1);
    assert_eq!(second.calls(), 1);
    assert_eq!(update.judgement, Some(Judgement::Validated));
}

async fn raw_faults_alone(bad: RawReply) {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let only = RawClient::new(vec![bad]);
    let critic = ladder(vec![FallbackList::from(ModelClient::new(
        only.clone(),
        ThinkingExpectation::NotExpected,
    ))]);
    let node = Pair::ladders(&rig, one(&generator), critic).build();

    let cause = Driver::failure(&node).await;

    assert!(!cause.is_empty());
    assert_eq!(only.calls(), 1);
}

macro_rules! failed_ending {
    ($falls:ident, $faults:ident, $reply:expr) => {
        #[tokio::test(start_paused = true)]
        async fn $falls() {
            raw_falls_back_from($reply).await;
        }

        #[tokio::test(start_paused = true)]
        async fn $faults() {
            raw_faults_alone($reply).await;
        }
    };
}

failed_ending!(
    given_a_refusal_ending_with_a_structured_block_that_parses_when_the_list_has_another_client_then_the_next_client_is_called,
    given_a_refusal_ending_with_a_structured_block_that_parses_when_the_list_is_used_up_then_the_instance_fails,
    RawReply::answer(true, "", ResponseEnding::refusal())
);

failed_ending!(
    given_a_filter_ending_with_a_structured_block_that_parses_when_the_list_has_another_client_then_the_next_client_is_called,
    given_a_filter_ending_with_a_structured_block_that_parses_when_the_list_is_used_up_then_the_instance_fails,
    RawReply::answer(true, "", ResponseEnding::filtering())
);

failed_ending!(
    given_an_exhausted_output_ending_with_a_structured_block_that_parses_when_the_list_has_another_client_then_the_next_client_is_called,
    given_an_exhausted_output_ending_with_a_structured_block_that_parses_when_the_list_is_used_up_then_the_instance_fails,
    RawReply::answer(false, "fix it", ResponseEnding::output_exhaustion())
);

#[tokio::test(start_paused = true)]
async fn given_a_list_of_three_when_two_calls_fail_then_the_third_client_gives_the_answer() {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let first = Scripted::new(vec![Reply::text("no structure")]);
    let second = Scripted::new(vec![Reply::answer(false, "")]);
    let third = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::ladders(
        &rig,
        one(&generator),
        ladder(vec![list(&[&first, &second, &third])]),
    )
    .build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!((first.calls(), second.calls(), third.calls()), (1, 1, 1));
}

#[tokio::test(start_paused = true)]
async fn given_a_list_of_two_when_both_calls_fail_then_the_last_error_is_a_fault_of_the_instance() {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let first = Scripted::new(vec![Reply::text("no structure")]);
    let second = Scripted::new(vec![Reply::answer(false, "")]);
    let node = Pair::ladders(&rig, one(&generator), list_of(&first, &second)).build();

    let cause = Driver::failure(&node).await;

    assert!(!cause.is_empty());
    assert_eq!((first.calls(), second.calls()), (1, 1));
}
