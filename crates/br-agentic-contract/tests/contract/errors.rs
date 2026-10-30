use br_agentic_contract::{PortError, ToolBlock, ToolContent};

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

#[test]
fn given_a_refused_request_when_a_tool_turns_it_into_its_result_then_the_content_has_the_error_flag_and_the_reason_as_text_and_there_is_no_fault()
 {
    let content = ToolContent::from_port_error(PortError::refused("not a recipient")).unwrap();

    assert!(content.is_error());
    assert_eq!(
        content.content(),
        &[ToolBlock::text("not a recipient").unwrap()]
    );
}

#[test]
fn given_a_refused_request_with_a_blank_reason_when_a_tool_turns_it_into_its_result_then_the_content_still_has_the_error_flag_and_a_text()
 {
    let content = ToolContent::from_port_error(PortError::refused("  ")).unwrap();

    assert!(content.is_error());
    assert_eq!(content.content().len(), 1);
}

#[test]
fn given_a_fault_when_a_tool_turns_it_into_its_result_then_it_stays_a_fault_with_the_same_cause() {
    let fault = ToolContent::from_port_error(PortError::fault("link down")).unwrap_err();

    assert_eq!(fault.cause().to_string(), "link down");
}
