use br_agentic_contract::{PortError, SessionError, ToolFault};
use br_agentic_nodes::{CallFailure, Fault};
use br_llm_clients_core::ClientError;

#[test]
fn given_a_port_fault_with_a_cause_when_the_detail_is_read_then_it_gives_the_fault_then_its_cause()
{
    let fault = Fault::Port(PortError::fault("the disk is full"));

    let detail = fault.detail();

    let outer = detail.find("a port failed").expect("the fault");
    let inner = detail.find("the disk is full").expect("the cause");
    assert!(outer < inner, "{detail}");
}

#[test]
fn given_a_tool_fault_when_the_detail_is_read_then_it_holds_the_tool_cause() {
    let fault = Fault::Tool(ToolFault::new("the tool broke"));

    let detail = fault.detail();

    assert!(detail.contains("a tool faulted"), "{detail}");
    assert!(detail.contains("the tool broke"), "{detail}");
}

#[test]
fn given_a_session_fault_over_a_port_cause_when_the_detail_is_read_then_the_whole_chain_is_given() {
    let fault = Fault::Session(SessionError::Port(PortError::fault("the disk is full")));

    let detail = fault.detail();

    assert!(detail.contains("the disk is full"), "{detail}");
}

#[test]
fn given_a_model_fault_when_the_detail_is_read_then_it_holds_the_client_error_text() {
    let fault = Fault::Model(CallFailure::Client(ClientError::Transport));

    let detail = fault.detail();

    assert!(
        detail.contains("the connection failed before the response headers"),
        "{detail}"
    );
}

#[test]
fn given_a_fault_without_a_cause_when_the_detail_is_read_then_it_is_the_text_of_the_fault() {
    let fault = Fault::Value("a blank prompt".to_owned());

    assert_eq!(fault.detail(), fault.to_string());
}
