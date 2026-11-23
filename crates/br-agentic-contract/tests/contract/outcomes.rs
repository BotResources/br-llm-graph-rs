use br_agentic_contract::{GeneratorCriticOutcome, Judgement, ReactOutcome, structured_reply};
use br_llm_messages::{
    ContentBlock, ModelResponse, OpaqueJson, ResponseEnding, ResponseMetadata, StructuredBlock,
};

use crate::common::*;

fn structured(json: &str) -> ModelResponse {
    structured_with(block_id(), json)
}

fn structured_with(id: br_llm_messages::BlockId, json: &str) -> ModelResponse {
    ModelResponse::new(
        entry_id(),
        vec![
            ContentBlock::Text(br_llm_messages::TextBlock::new(block_id(), "words").unwrap()),
            ContentBlock::StructuredContent(
                StructuredBlock::new(id, OpaqueJson::parse(json).unwrap()).unwrap(),
            ),
        ],
        ResponseEnding::finished(),
        ResponseMetadata::new(),
    )
    .unwrap()
}

#[test]
fn given_a_response_with_a_structured_block_when_the_reply_is_asked_then_it_is_the_first_structured_block()
 {
    let id = block_id();
    let response = structured_with(id, r#"{"answer":"yes"}"#);

    let reply = structured_reply(&response).unwrap();

    assert_eq!(reply.id(), id);
}

#[test]
fn given_a_response_without_a_structured_block_when_the_reply_is_asked_then_it_is_nothing() {
    assert!(structured_reply(&response_text(entry_id(), "plain")).is_none());
}

#[test]
fn given_the_parts_when_a_react_outcome_is_built_by_any_caller_then_it_gives_them_back() {
    let response = structured(r#"{"n":1}"#);

    let outcome = ReactOutcome::new(response.clone(), 3, true);

    assert_eq!(outcome.response(), &response);
    assert_eq!(outcome.steps(), 3);
    assert!(outcome.step_limit_reached());
    assert!(outcome.structured_reply().is_some());
}

#[test]
fn given_a_react_outcome_with_a_plain_response_then_it_has_no_structured_reply() {
    let outcome = ReactOutcome::new(response_text(entry_id(), "done"), 1, false);

    assert!(outcome.structured_reply().is_none());
    assert!(!outcome.step_limit_reached());
}

#[test]
fn given_the_parts_when_a_generator_critic_outcome_is_built_by_any_caller_then_it_gives_them_back()
{
    let response = structured(r#"{"n":2}"#);

    let outcome = GeneratorCriticOutcome::new(response.clone(), Judgement::Validated, 2, false);

    assert_eq!(outcome.response(), &response);
    assert_eq!(outcome.judgement(), Judgement::Validated);
    assert_eq!(outcome.iterations(), 2);
    assert!(!outcome.step_limit_reached());
    assert!(outcome.structured_reply().is_some());
}

#[test]
fn given_the_three_judgements_then_they_are_distinct() {
    assert_ne!(Judgement::Validated, Judgement::Rejected);
    assert_ne!(Judgement::Rejected, Judgement::NotJudged);
    assert_ne!(Judgement::Validated, Judgement::NotJudged);
}
