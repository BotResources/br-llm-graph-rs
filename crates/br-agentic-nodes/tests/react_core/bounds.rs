use br_agentic_nodes::{EmptyList, FallbackList, InvalidLimit, StepLimit};

#[test]
fn given_a_step_limit_of_zero_when_it_is_built_then_it_is_refused() {
    assert_eq!(StepLimit::new(0), Err(InvalidLimit::Steps));
}

#[test]
fn given_a_step_limit_of_one_when_it_is_built_then_it_is_accepted() {
    assert_eq!(StepLimit::new(1).unwrap().get(), 1);
}

#[test]
fn given_no_client_when_a_fallback_list_is_built_then_it_is_refused() {
    assert_eq!(FallbackList::new(Vec::new()).unwrap_err(), EmptyList);
}
