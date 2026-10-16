//! A generation cut by `OnLimit::End` is told by its closing step: the stop
//! reason `other(tool_round_limit)` on a step the limit node wrote (no model,
//! no usage), and only when the generator's round limit is `End`. A
//! provider's reply that happens to carry that stop reason is an answer.

use std::sync::Arc;

use br_llm_messages::{AssistantBlock, ModelId, Step, StopReason, Text, Usage};

use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::given::Model;
use crate::react::seats::GeneratorSeat;
use crate::react::test_support::*;

/// A complete answer whose stop reason is the one the limit's closing step
/// carries, with or without a model.
fn lookalike(model: Option<&str>) -> Step {
    lookalike_with(None, model)
}

fn lookalike_with(usage: Option<Usage>, model: Option<&str>) -> Step {
    Step::new(
        vec![AssistantBlock::Text {
            text: Text::new("A real, complete answer.").unwrap(),
        }],
        StopReason::Other {
            reason: "tool_round_limit".to_owned(),
        },
        usage,
        model.map(|model| ModelId::new(model).unwrap()),
    )
    .unwrap()
}

#[tokio::test]
async fn given_a_provider_reply_whose_stop_reason_is_the_limits_without_a_round_limit_when_run_then_it_is_judged()
 {
    // R3b's probe: the mock gives the step the model of its settings.
    let generator_client = mock(vec![lookalike(None), text_step("second answer")]);
    let critic_client = mock(vec![verdict(true, ""), verdict(true, "")]);
    let graph = duo(&generator_client, &critic_client).graph().unwrap();

    let state = ended(start(&graph, Vec::new()).await, "validated");

    assert_eq!(
        state.str(&key("answer")).unwrap(),
        "A real, complete answer."
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(requests(&critic_client).len(), 1);
}

#[tokio::test]
async fn given_a_provider_reply_whose_stop_reason_is_the_limits_with_an_end_limit_when_run_then_it_is_judged()
 {
    // A step a client gives carries a model: it is not the limit's.
    let generator_client = mock(vec![lookalike(None)]);
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = GeneratorCritic::new(
        cut_generator(&generator_client),
        critic(&critic_client),
        Count::Fixed(2),
    )
    .graph()
    .unwrap();

    let state = ended(start(&graph, Vec::new()).await, "validated");

    assert_eq!(
        state.str(&key("answer")).unwrap(),
        "A real, complete answer."
    );
    assert_eq!(requests(&critic_client).len(), 1);
}

#[tokio::test]
async fn given_a_step_without_model_and_the_limits_stop_reason_from_a_generator_without_an_end_limit_when_run_then_it_is_judged()
 {
    // Only an `End` limit closes a generation: without one, even a step
    // bearing no model (from a client that serves steps as they are) is an
    // answer.
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            model: Model::client(Arc::new(LaxClient::new(vec![lookalike(None)]))),
            ..generator(&mock(Vec::new()))
        },
        critic(&critic_client),
        Count::Fixed(2),
    )
    .graph()
    .unwrap();

    let state = ended(start(&graph, Vec::new()).await, "validated");

    assert_eq!(
        state.str(&key("answer")).unwrap(),
        "A real, complete answer."
    );
    assert_eq!(requests(&critic_client).len(), 1);
}

#[tokio::test]
async fn given_a_step_without_model_but_with_a_usage_and_the_limits_stop_reason_under_an_end_limit_when_run_then_it_is_judged()
 {
    // The limit node writes no usage: a step a client counted tokens for is
    // not its closing step, even from a client that serves steps without a
    // model.
    let usage = Usage {
        input_tokens: 12,
        output_tokens: 5,
        ..Usage::default()
    };
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            model: Model::client(Arc::new(LaxClient::new(vec![lookalike_with(
                Some(usage),
                None,
            )]))),
            ..cut_generator(&mock(Vec::new()))
        },
        critic(&critic_client),
        Count::Fixed(2),
    )
    .graph()
    .unwrap();

    let state = ended(start(&graph, Vec::new()).await, "validated");

    assert_eq!(
        state.str(&key("answer")).unwrap(),
        "A real, complete answer."
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(requests(&critic_client).len(), 1);
}

#[tokio::test]
async fn given_a_generation_the_end_limit_cut_when_run_then_its_closing_step_is_still_an_empty_answer()
 {
    // Positive control: the closing step the limit node writes.
    let generator_client = mock(vec![
        tool_call_step("c1", "echo", serde_json::json!({})),
        tool_call_step("c2", "echo", serde_json::json!({})),
        text_step("full"),
    ]);
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = GeneratorCritic::new(
        cut_generator(&generator_client),
        critic(&critic_client),
        Count::Fixed(2),
    )
    .graph()
    .unwrap();

    let state = ended(start(&graph, Vec::new()).await, "validated");

    assert_eq!(
        shape(state.transcript()),
        [
            "user".to_owned(),
            format!("writer:[5 items] {CUT}"),
            empty_critique(),
            "writer:full".to_owned(),
            "reviewer:Accepted.".to_owned()
        ]
    );
    assert_eq!(requests(&critic_client).len(), 1);
}
