use std::sync::Arc;

use br_agentic_contract::PortFault;
use br_agentic_nodes::{
    CriticRequest, CriticRequestPort, CriticRequestSource, GeneratorCritic, GeneratorCriticSpec,
    RoundLimit,
};
use br_llm_clients_core::BoxFuture;
use br_llm_messages::{HumanId, PlainText};

use crate::common::*;

#[tokio::test(start_paused = true)]
async fn given_an_answer_that_cannot_be_unwrapped_when_the_ladder_has_a_next_client_then_the_next_client_serves()
 {
    let rig = Rig::new();
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let bad = Scripted::new(vec![Reply::text("not a structure")]);
    let good = Scripted::new(vec![Reply::answer(true, "ok")]);
    let node = rig.generator_critic(one(&generator), ladder(vec![list(&[&bad, &good])]), 3);

    let update = Driver::result(&node).await;

    assert_eq!(update.verdict, Some(true));
    assert_eq!((bad.calls(), good.calls()), (1, 1));
}

#[tokio::test(start_paused = true)]
async fn given_a_blank_message_without_a_validation_when_the_list_is_used_up_then_the_instance_fails()
 {
    let rig = Rig::new();
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(false, "   ")]);
    let node = rig.generator_critic(one(&generator), one(&critic), 3);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("blank message"), "{cause}");
    let conversation = conversation(&rig.hub, &rig.thread);
    assert!(agent_mails(&conversation).is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_blank_message_with_a_validation_when_the_critic_answers_then_the_verdict_is_taken()
{
    let rig = Rig::new();
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = rig.generator_critic(one(&generator), one(&critic), 3);

    let update = Driver::result(&node).await;

    assert_eq!(update.verdict, Some(true));
}

#[tokio::test(start_paused = true)]
async fn given_a_thinking_field_in_the_answer_when_the_critic_does_not_validate_then_only_the_message_is_stored()
 {
    let rig = Rig::new();
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::Structured(
            "{\"thinking\":\"private thoughts\",\"is_validated\":false,\"message\":\"fix it\"}"
                .to_owned(),
        ),
        Reply::answer(true, "ok"),
    ]);
    let node = rig
        .generator_critic(one(&generator), one(&critic), 3)
        .with_critic_thinking_field();

    Driver::result(&node).await;

    let conversation = conversation(&rig.hub, &rig.thread);
    let stored = format!("{conversation:?}");
    assert!(stored.contains("fix it"));
    assert!(!stored.contains("private thoughts"));
    assert!(format!("{:?}", critic.requests()[0].output()).contains("thinking"));
}

#[tokio::test(start_paused = true)]
async fn given_no_thinking_field_in_the_build_when_the_critic_is_called_then_the_structure_holds_two_keys()
 {
    let rig = Rig::new();
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("one")]);
    let critic = Scripted::new(vec![Reply::answer(true, "ok")]);
    let node = rig.generator_critic(one(&generator), one(&critic), 3);

    Driver::result(&node).await;

    let shape = format!("{:?}", critic.requests()[0].output());
    assert!(shape.contains("is_validated"));
    assert!(shape.contains("message"));
    assert!(!shape.contains("thinking"));
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_client_that_should_think_when_the_answer_comes_without_thinking_then_the_next_client_serves()
 {
    let rig = Rig::new();
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let silent = Scripted::new(vec![Reply::answer(true, "no thought")]);
    let thinker = Scripted::new(vec![Reply::thinking(Reply::answer(true, "thought"))]);
    let critic = ladder(vec![
        br_agentic_nodes::FallbackList::new(vec![
            br_agentic_nodes::ModelClient::new(
                silent.clone(),
                br_agentic_nodes::ThinkingExpectation::Expected,
            ),
            br_agentic_nodes::ModelClient::new(
                thinker.clone(),
                br_agentic_nodes::ThinkingExpectation::Expected,
            ),
        ])
        .unwrap(),
    ]);
    let node = rig.generator_critic(one(&generator), critic, 3);

    let update = Driver::result(&node).await;

    assert_eq!(update.verdict, Some(true));
    assert_eq!((silent.calls(), thinker.calls()), (1, 1));
}

struct TextFromState;

impl CriticRequestPort<Doc> for TextFromState {
    fn request<'a>(&'a self, snapshot: &'a Doc) -> BoxFuture<'a, Result<CriticRequest, PortFault>> {
        Box::pin(async move {
            Ok(CriticRequest {
                text: PlainText::new(format!("judge it, note {}", snapshot.note)).unwrap(),
                human: HumanId::from_uuid(uuid()).unwrap(),
            })
        })
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_request_from_a_port_when_the_critic_is_called_then_the_text_comes_from_the_port_with_the_snapshot()
 {
    let rig = Rig::new();
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "ok")]);
    let node = GeneratorCritic::new(
        "pair",
        rig.ports(),
        rig.settings.clone(),
        GeneratorCriticSpec {
            generator: one(&generator),
            critic: one(&critic),
            round_limit: RoundLimit::new(2).unwrap(),
            critic_request: CriticRequestSource::Port(Arc::new(TextFromState)),
        },
    );

    Driver::result(&node).await;

    assert!(history_text(&critic.requests()[0]).contains("judge it, note start"));
}
