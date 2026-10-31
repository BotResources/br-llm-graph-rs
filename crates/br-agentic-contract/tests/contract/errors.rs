use br_agentic_contract::PortError;

#[test]
fn given_a_refused_request_then_the_error_has_the_reason_as_text() {
    let error = PortError::refused("no recipient in to");

    assert!(error.is_refusal());
    assert_eq!(error.reason().unwrap().as_str(), "no recipient in to");
    assert!(error.to_string().contains("no recipient in to"));
}

#[test]
fn given_a_fault_then_the_error_keeps_an_opaque_cause_and_is_not_a_refusal() {
    use std::error::Error;

    let error = PortError::fault("disk gone");

    assert!(!error.is_refusal());
    assert!(error.reason().is_none());
    assert_eq!(error.source().unwrap().to_string(), "disk gone");
}
