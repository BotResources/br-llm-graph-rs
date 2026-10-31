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
fn given_a_refused_request_of_the_inbox_when_a_tool_turns_it_into_its_result_then_the_content_has_the_error_flag_and_the_reason_as_text()
 {
    let content = ToolContent::from_inbox_error(PortError::refused("not a recipient"));

    assert!(content.is_error());
    assert_eq!(
        content.content(),
        &[ToolBlock::text("not a recipient").unwrap()]
    );
}

#[test]
fn given_a_refused_request_of_the_inbox_with_a_blank_reason_when_a_tool_turns_it_into_its_result_then_the_content_still_has_the_error_flag_and_a_text()
 {
    let content = ToolContent::from_inbox_error(PortError::refused("  "));

    assert!(content.is_error());
    assert_eq!(content.content().len(), 1);
}

#[test]
fn given_a_fault_of_the_inbox_when_a_tool_turns_it_into_its_result_then_the_content_has_the_error_flag_and_a_text_without_the_cause_and_the_run_goes_on()
 {
    let content = ToolContent::from_inbox_error(PortError::fault("link down at 10.0.0.7"));

    assert!(content.is_error());
    let [ToolBlock::Text(text)] = content.content() else {
        panic!("one text block")
    };
    assert!(!text.as_str().trim().is_empty());
    assert!(!text.as_str().contains("10.0.0.7"));
}
