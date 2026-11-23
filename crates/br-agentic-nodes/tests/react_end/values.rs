use std::time::Duration;

use br_agentic_nodes::{
    EmptyList, FallbackList, InvalidLimit, Ladder, ProgrammaticRefusalLimit, RoundLimit, StepLimit,
    Timeouts,
};

use crate::common::*;

#[test]
fn given_a_step_limit_of_zero_when_it_is_built_then_it_is_refused_and_one_is_the_least() {
    assert_eq!(StepLimit::new(0), Err(InvalidLimit::Steps));
    assert_eq!(StepLimit::new(1).unwrap().get(), 1);
}

#[test]
fn given_a_round_limit_of_zero_when_it_is_built_then_it_is_refused_and_one_is_the_least() {
    assert_eq!(RoundLimit::new(0), Err(InvalidLimit::Rounds));
    assert_eq!(RoundLimit::new(1).unwrap().get(), 1);
}

#[test]
fn given_a_programmatic_refusal_limit_of_zero_when_it_is_built_then_it_is_refused_and_one_is_the_least()
 {
    assert_eq!(
        ProgrammaticRefusalLimit::new(0),
        Err(InvalidLimit::Refusals)
    );
    assert_eq!(ProgrammaticRefusalLimit::new(1).unwrap().get(), 1);
}

#[test]
fn given_a_zero_timeout_when_the_timeouts_are_built_then_they_are_refused() {
    let second = Duration::from_secs(1);
    assert_eq!(
        Timeouts::new(Duration::ZERO, second),
        Err(InvalidLimit::Timeout)
    );
    assert_eq!(
        Timeouts::new(second, Duration::ZERO),
        Err(InvalidLimit::Timeout)
    );
    let timeouts = Timeouts::new(second, Duration::from_millis(1)).unwrap();
    assert_eq!(timeouts.model_call(), second);
    assert_eq!(timeouts.tool_call(), Duration::from_millis(1));
}

#[test]
fn given_no_client_when_a_fallback_list_or_a_ladder_is_built_then_it_is_refused() {
    assert!(matches!(FallbackList::new(Vec::new()), Err(EmptyList)));
    assert!(matches!(Ladder::new(Vec::new()), Err(EmptyList)));
}

#[test]
fn given_one_client_when_a_fallback_list_is_built_then_it_holds_it() {
    let model = Scripted::new(vec![]);
    let fallback = list(&[&model]);
    assert_eq!(fallback.len(), 1);
    assert!(!fallback.is_empty());
}

#[test]
fn given_a_ladder_of_three_entries_when_a_position_is_past_the_end_then_the_last_entry_serves_it() {
    let model = Scripted::new(vec![]);
    let ladder = ladder(vec![
        list(&[&model]),
        list(&[&model, &model]),
        list(&[&model, &model, &model]),
    ]);

    assert_eq!(ladder.len(), 3);
    assert_eq!(ladder.entry(0).len(), 1);
    assert_eq!(ladder.entry(2).len(), 3);
    assert_eq!(ladder.entry(3).len(), 3);
    assert_eq!(ladder.entry(100).len(), 3);
    assert_eq!(ladder.entry(u32::MAX).len(), 3);
}

#[test]
fn given_a_ladder_of_one_entry_when_any_position_is_asked_then_that_entry_serves_it() {
    let model = Scripted::new(vec![]);
    let ladder = ladder(vec![list(&[&model, &model])]);

    assert_eq!(ladder.entry(0).len(), 2);
    assert_eq!(ladder.entry(7).len(), 2);
}
