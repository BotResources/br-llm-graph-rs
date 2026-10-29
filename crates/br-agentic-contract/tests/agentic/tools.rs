use std::sync::Arc;

use br_agentic_contract::{
    DuplicateTool, ResultStream, Tool, ToolCode, ToolContent, ToolFault, ToolSet,
};
use br_graph_contract::{CancelToken, CancelView};
use br_llm_clients_core::{BoxFuture, Parameters, ToolSpec};
use br_llm_messages::{OpaqueJson, ToolName};

use crate::common::*;

enum Behavior {
    Echo,
    ExpectedError,
    Fault,
    ObserveCancel,
}

struct Code(Behavior);

impl ToolCode for Code {
    fn call<'a>(
        &'a self,
        arguments: &'a OpaqueJson,
        cancel: CancelView,
        result: &'a mut dyn ResultStream,
    ) -> BoxFuture<'a, Result<ToolContent, ToolFault>> {
        Box::pin(async move {
            match self.0 {
                Behavior::Echo => {
                    result.send("fragment".to_owned()).await;
                    let text = arguments.to_json_string().unwrap();
                    Ok(ToolContent::new(vec![text_block(&text)], false))
                }
                Behavior::ExpectedError => {
                    ToolContent::error_text(block_id(), "cannot read the document")
                        .map_err(ToolFault::new)
                }
                Behavior::Fault => Err(ToolFault::new("disk gone")),
                Behavior::ObserveCancel => {
                    let seen = cancel.is_triggered();
                    Ok(ToolContent::new(vec![text_block(&seen.to_string())], false))
                }
            }
        })
    }
}

fn tool(name: &str, behavior: Behavior) -> Tool {
    let spec = ToolSpec::new(
        ToolName::new(name).unwrap(),
        "a tool",
        Parameters::new(Vec::new()).unwrap(),
    )
    .unwrap();
    Tool::new(spec, Arc::new(Code(behavior)))
}

struct Sink(Vec<String>);

impl ResultStream for Sink {
    fn send<'a>(&'a mut self, fragment: String) -> BoxFuture<'a, ()> {
        self.0.push(fragment);
        Box::pin(async {})
    }
}

#[tokio::test]
async fn given_a_tool_when_it_is_called_then_it_gets_the_arguments_the_cancel_view_and_the_result_stream_and_the_whole_content_is_the_truth()
 {
    let tool = tool("echo", Behavior::Echo);
    let arguments = OpaqueJson::parse(r#"{"q":"rust"}"#).unwrap();
    let mut sink = Sink(Vec::new());

    let content = tool
        .call(&arguments, CancelToken::new().view(), &mut sink)
        .await
        .unwrap();

    assert_eq!(sink.0, vec!["fragment".to_owned()]);
    assert!(format!("{:?}", content.content()).contains("rust"));
    assert!(!content.is_error());
    assert!(!format!("{:?}", content.content()).contains("fragment"));
}

#[tokio::test]
async fn given_a_tool_that_meets_an_expected_error_when_it_ends_then_the_content_has_the_error_flag_and_a_text_and_it_does_not_fault()
 {
    let tool = tool("read", Behavior::ExpectedError);
    let arguments = OpaqueJson::parse("{}").unwrap();

    let content = tool
        .call(&arguments, CancelToken::new().view(), &mut Sink(Vec::new()))
        .await
        .unwrap();

    assert!(content.is_error());
    assert!(format!("{:?}", content.content()).contains("cannot read the document"));
}

#[tokio::test]
async fn given_a_tool_that_faults_when_it_ends_then_the_fault_carries_an_opaque_cause() {
    let tool = tool("broken", Behavior::Fault);
    let arguments = OpaqueJson::parse("{}").unwrap();

    let fault = tool
        .call(&arguments, CancelToken::new().view(), &mut Sink(Vec::new()))
        .await
        .unwrap_err();

    assert_eq!(fault.cause().to_string(), "disk gone");
}

#[tokio::test]
async fn given_a_triggered_cancel_token_when_a_tool_is_called_then_it_reads_the_read_only_view_as_triggered()
 {
    let tool = tool("watch", Behavior::ObserveCancel);
    let token = CancelToken::new();
    token.handle().trigger();
    let arguments = OpaqueJson::parse("{}").unwrap();

    let content = tool
        .call(&arguments, token.view(), &mut Sink(Vec::new()))
        .await
        .unwrap();

    assert!(format!("{:?}", content.content()).contains("true"));
}

#[tokio::test]
async fn given_a_tool_content_when_the_instance_builds_the_result_then_it_holds_the_internal_id_the_call_and_the_name()
 {
    let content = ToolContent::new(vec![text_block("found")], false);
    let (id, call) = (block_id(), block_id());

    let result = content
        .into_result(id, call, ToolName::new("search").unwrap())
        .unwrap();

    assert_eq!(result.id(), id);
    assert_eq!(result.call(), call);
    assert_eq!(result.tool_name().as_str(), "search");
    assert!(!result.is_error());
}

#[test]
fn given_a_name_that_appears_twice_among_the_tools_when_the_set_is_built_then_it_is_refused() {
    let error = ToolSet::new(
        vec![tool("search", Behavior::Echo), tool("read", Behavior::Echo)],
        vec![tool("search", Behavior::Echo)],
    )
    .unwrap_err();

    assert_eq!(
        error,
        DuplicateTool {
            name: "search".to_owned()
        }
    );
}

#[test]
fn given_a_mail_tool_that_repeats_a_tool_name_when_the_set_is_built_then_the_mail_tools_count_in_the_check()
 {
    let error = ToolSet::new(
        vec![tool("send_mail", Behavior::Echo)],
        vec![
            tool("send_mail", Behavior::Echo),
            tool("get_files", Behavior::Echo),
        ],
    )
    .unwrap_err();

    assert_eq!(error.name, "send_mail");
}

#[test]
fn given_unique_names_when_the_set_is_built_then_every_tool_is_found_by_name() {
    let set = ToolSet::new(
        vec![tool("search", Behavior::Echo)],
        vec![tool("send_mail", Behavior::Echo)],
    )
    .unwrap();

    assert_eq!(set.tools().len(), 2);
    assert!(set.find("send_mail").is_some());
    assert!(set.find("missing").is_none());
    assert_eq!(set.specs().len(), 2);
}
