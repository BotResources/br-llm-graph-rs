use std::time::Duration;

use br_agentic_nodes::{
    EmptyList, FallbackList, InvalidLimit, Ladder, RoundLimit, StepLimit, Timeouts, WARNING_WINDOW,
    check_thinking, holds_thinking, warning_text,
};

use crate::common::*;

#[test]
fn given_a_zero_limit_when_it_is_built_then_it_is_refused() {
    assert_eq!(StepLimit::new(0), Err(InvalidLimit::Steps));
    assert_eq!(RoundLimit::new(0), Err(InvalidLimit::Rounds));
    assert_eq!(
        Timeouts::new(Duration::ZERO, Duration::from_secs(1)),
        Err(InvalidLimit::Timeout)
    );
    assert_eq!(
        Timeouts::new(Duration::from_secs(1), Duration::ZERO),
        Err(InvalidLimit::Timeout)
    );
    assert_eq!(StepLimit::new(7).unwrap().get(), 7);
    assert_eq!(RoundLimit::new(3).unwrap().get(), 3);
}

#[test]
fn given_an_empty_list_when_a_fallback_list_or_a_ladder_is_built_then_it_is_refused() {
    assert_eq!(FallbackList::new(Vec::new()).unwrap_err(), EmptyList);
    assert_eq!(Ladder::new(Vec::new()).unwrap_err(), EmptyList);
}

#[test]
fn given_a_ladder_when_a_position_beyond_its_end_is_asked_then_the_last_entry_answers() {
    let a = Scripted::new(vec![]);
    let b = Scripted::new(vec![]);
    let ladder = ladder(vec![list(&[&a]), list(&[&b, &b])]);

    assert_eq!(ladder.entry(0).len(), 1);
    assert_eq!(ladder.entry(1).len(), 2);
    assert_eq!(ladder.entry(2).len(), 2);
    assert_eq!(ladder.entry(u32::MAX).len(), 2);
    assert_eq!(ladder.len(), 2);
}

#[test]
fn given_the_steps_that_remain_when_the_warning_is_asked_then_it_shows_from_ten_steps_down_and_never_at_zero()
 {
    assert_eq!(WARNING_WINDOW, 10);
    assert!(warning_text(11).is_none());
    assert!(warning_text(10).unwrap().contains("10 steps left"));
    assert!(warning_text(2).unwrap().contains("2 steps left"));
    assert!(warning_text(1).unwrap().contains("ask for no tool call"));
    assert!(warning_text(0).is_none());
}

fn response(blocks: Vec<br_llm_messages::ContentBlock>) -> br_llm_messages::ModelResponse {
    br_llm_messages::ModelResponse::new(
        entry_id(),
        blocks,
        br_llm_messages::ResponseEnding::finished(),
        br_llm_messages::ResponseMetadata::new(),
    )
    .unwrap()
}

#[test]
fn given_a_response_with_or_without_thinking_when_the_rule_is_checked_then_only_a_client_that_should_think_refuses()
 {
    use br_llm_messages::{ContentBlock, TextBlock, ThinkingBlock};
    let text = ContentBlock::Text(TextBlock::new(block_id(), "a").unwrap());
    let thought = ContentBlock::Thinking(ThinkingBlock::new(block_id(), "hm").unwrap());
    let silent = response(vec![text.clone()]);
    let thoughtful = response(vec![thought, text]);
    let model = Scripted::new(vec![]);
    let plain = br_agentic_nodes::ModelClient::new(model.clone());
    let expecting = br_agentic_nodes::ModelClient::new(model).expecting_thinking();

    assert!(!holds_thinking(&silent));
    assert!(holds_thinking(&thoughtful));
    assert!(check_thinking(&plain, &silent).is_ok());
    assert!(check_thinking(&expecting, &thoughtful).is_ok());
    assert!(matches!(
        check_thinking(&expecting, &silent),
        Err(br_agentic_nodes::CallFailure::NoThinking)
    ));
}
