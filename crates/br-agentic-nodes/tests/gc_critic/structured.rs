use std::sync::Arc;

use br_llm_messages::{IncomingEntry, OpaqueJson, PlainBlock, Turn};

use crate::pair::Pair;
use crate::prelude::*;
use crate::seen::shape_names;

fn giving() -> Arc<Shapes> {
    Arc::new(Shapes::giving(reply_shape()))
}

fn shows_structured(request: &br_llm_clients_core::ClientRequest, json: &str) -> bool {
    let expected = OpaqueJson::parse(json).unwrap();
    request
        .history()
        .turns()
        .iter()
        .filter_map(|turn| match turn {
            Turn::InputTurn(input) => Some(input.entries()),
            _ => None,
        })
        .flatten()
        .filter_map(|entry| match entry {
            IncomingEntry::AgentOutput(output) => Some(output.content()),
            _ => None,
        })
        .flatten()
        .any(|block| matches!(block, PlainBlock::StructuredContent(structured) if structured.value() == &expected))
}

#[tokio::test(start_paused = true)]
async fn given_a_shape_when_the_pair_starts_then_the_port_is_called_for_the_generator_only() {
    let rig = Rig::new();
    let shapes = giving();
    let generator = Scripted::new(vec![Reply::Structured(r#"{"answer":"yes"}"#.to_owned())]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::new(&rig, &generator, &critic)
        .ports(rig.ports().with_output_schema(shapes.clone()))
        .build();

    Driver::result(&node).await;

    assert_eq!(shapes.labels(), vec!["generator"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_shape_when_the_pair_runs_then_the_generator_asks_for_it_and_the_critic_keeps_its_own_shape()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::Structured(r#"{"answer":"yes"}"#.to_owned())]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::new(&rig, &generator, &critic)
        .ports(rig.ports().with_output_schema(giving()))
        .build();

    Driver::result(&node).await;

    assert_eq!(asked_shape(&generator.requests()[0]), Some(&reply_shape()));
    assert_eq!(
        shape_names(&critic.requests()[0]),
        vec!["thinking", "is_validated", "message"]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_shape_when_the_critic_is_called_then_it_sees_the_structured_reply_of_the_generator()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::Structured(r#"{"answer":"yes"}"#.to_owned())]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::new(&rig, &generator, &critic)
        .ports(rig.ports().with_output_schema(giving()))
        .build();

    Driver::result(&node).await;

    assert!(shows_structured(
        &critic.requests()[0],
        r#"{"answer":"yes"}"#
    ));
}

#[tokio::test(start_paused = true)]
async fn given_a_shape_when_the_critic_validates_then_the_outcome_gives_the_structured_reply_of_the_generator()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::Structured(r#"{"answer":"yes"}"#.to_owned())]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::new(&rig, &generator, &critic)
        .ports(rig.ports().with_output_schema(giving()))
        .build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    let outcome = rig.outcome.pair_outcomes().remove(0);
    let reply = outcome.structured_reply().expect("a structured reply");
    assert_eq!(
        reply.value(),
        &OpaqueJson::parse(r#"{"answer":"yes"}"#).unwrap()
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_shape_when_the_generator_ends_a_pass_with_no_structured_block_then_the_critic_is_not_called_and_the_instance_fails()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("plain")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::new(&rig, &generator, &critic)
        .ports(rig.ports().with_output_schema(giving()))
        .build();

    let failure = Driver::failure(&node).await;

    assert!(failure.contains("structured block"), "{failure}");
    assert_eq!(critic.calls(), 0);
}
