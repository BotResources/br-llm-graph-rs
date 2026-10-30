use std::sync::Arc;

use br_agentic_contract::{
    DuplicateTool, PortFuture, ResultStream, Tool, ToolBlock, ToolCode, ToolContent, ToolFault,
    ToolSet,
};
use br_graph_contract::{CancelToken, CancelView};
use br_llm_clients_core::{Parameters, ToolSpec};
use br_llm_messages::{OpaqueJson, PlainBlock, ToolName};

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
    ) -> PortFuture<'a, Result<ToolContent, ToolFault>> {
        Box::pin(async move {
            match self.0 {
                Behavior::Echo => {
                    result.send("fragment".to_owned()).await;
                    let text = arguments.to_json_string().unwrap();
                    Ok(ToolContent::new(
                        vec![ToolBlock::text(text).unwrap()],
                        false,
                    ))
                }
                Behavior::ExpectedError => {
                    ToolContent::error_text("cannot read the document").map_err(ToolFault::new)
                }
                Behavior::Fault => Err(ToolFault::new("disk gone")),
                Behavior::ObserveCancel => {
                    let seen = cancel.is_triggered();
                    Ok(ToolContent::new(
                        vec![ToolBlock::text(seen.to_string()).unwrap()],
                        false,
                    ))
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
    fn send<'a>(&'a mut self, fragment: String) -> PortFuture<'a, ()> {
        self.0.push(fragment);
        Box::pin(async {})
    }
}

fn text_of(content: &ToolContent) -> String {
    match &content.content()[0] {
        ToolBlock::Text(text) => text.as_str().to_owned(),
        other => panic!("not a text: {other:?}"),
    }
}

#[tokio::test]
async fn given_a_tool_when_it_is_called_then_it_gets_the_arguments_and_the_result_stream_and_the_whole_content_is_the_truth()
 {
    let tool = tool("echo", Behavior::Echo);
    let arguments = OpaqueJson::parse(r#"{"q":"rust"}"#).unwrap();
    let mut sink = Sink(Vec::new());

    let content = tool
        .call(&arguments, CancelToken::new().view(), &mut sink)
        .await
        .unwrap();

    assert_eq!(sink.0, vec!["fragment".to_owned()]);
    assert!(text_of(&content).contains("rust"));
    assert!(!text_of(&content).contains("fragment"));
    assert!(!content.is_error());
}

#[tokio::test]
async fn given_a_tool_that_sends_no_fragment_when_it_ends_then_the_whole_content_is_still_given() {
    let tool = tool("quiet", Behavior::ObserveCancel);
    let mut sink = Sink(Vec::new());

    let content = tool
        .call(
            &OpaqueJson::parse("{}").unwrap(),
            CancelToken::new().view(),
            &mut sink,
        )
        .await
        .unwrap();

    assert!(sink.0.is_empty());
    assert_eq!(text_of(&content), "false");
}

#[tokio::test]
async fn given_a_tool_that_meets_an_expected_error_when_it_ends_then_the_content_has_the_error_flag_and_a_text_and_it_does_not_fault()
 {
    let tool = tool("read", Behavior::ExpectedError);
    let mut sink = Sink(Vec::new());

    let content = tool
        .call(
            &OpaqueJson::parse("{}").unwrap(),
            CancelToken::new().view(),
            &mut sink,
        )
        .await
        .unwrap();

    assert!(content.is_error());
    assert_eq!(text_of(&content), "cannot read the document");
}

#[tokio::test]
async fn given_a_tool_that_faults_when_it_ends_then_the_call_gives_a_fault_with_an_opaque_cause() {
    let tool = tool("broken", Behavior::Fault);
    let mut sink = Sink(Vec::new());

    let fault = tool
        .call(
            &OpaqueJson::parse("{}").unwrap(),
            CancelToken::new().view(),
            &mut sink,
        )
        .await
        .unwrap_err();

    assert_eq!(fault.cause().to_string(), "disk gone");
}

#[tokio::test]
async fn given_a_triggered_cancel_token_when_a_tool_is_called_then_it_sees_the_read_only_view_of_the_trigger()
 {
    let tool = tool("watch", Behavior::ObserveCancel);
    let token = CancelToken::new();
    let handle = token.handle();
    let mut sink = Sink(Vec::new());
    handle.trigger();

    let content = tool
        .call(&OpaqueJson::parse("{}").unwrap(), token.view(), &mut sink)
        .await
        .unwrap();

    assert_eq!(text_of(&content), "true");
}

#[test]
fn given_a_tool_content_when_the_instance_builds_the_result_then_it_gives_the_internal_id_the_name_of_the_call_and_an_identity_to_each_block()
 {
    let content = ToolContent::new(
        vec![
            ToolBlock::text("one").unwrap(),
            ToolBlock::text("two").unwrap(),
        ],
        false,
    );
    let (id, call) = (block_id(), block_id());

    let result = content
        .into_result(id, call, ToolName::new("lookup").unwrap(), block_id)
        .unwrap();

    assert_eq!((result.id(), result.call()), (id, call));
    assert_eq!(result.tool_name().as_str(), "lookup");
    assert!(!result.is_error());
    let ids: Vec<_> = result.content().iter().map(PlainBlock::id).collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
}

#[test]
fn given_a_tool_content_with_the_error_flag_when_the_instance_builds_the_result_then_the_result_has_the_error_flag()
 {
    let content = ToolContent::error_text("bad arguments").unwrap();

    let result = content
        .into_result(
            block_id(),
            block_id(),
            ToolName::new("lookup").unwrap(),
            block_id,
        )
        .unwrap();

    assert!(result.is_error());
}

#[test]
fn given_a_blank_text_when_a_tool_block_is_made_then_it_is_refused() {
    assert!(ToolBlock::text("  ").is_err());
    assert!(ToolContent::error_text("").is_err());
}

#[test]
fn given_a_name_that_appears_twice_among_the_tools_when_the_tool_set_is_made_then_it_is_refused() {
    let error = ToolSet::new(
        vec![
            tool("search", Behavior::Echo),
            tool("search", Behavior::Echo),
        ],
        Vec::new(),
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
fn given_a_mail_tool_with_the_name_of_another_tool_when_the_tool_set_is_made_then_it_is_refused() {
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
fn given_distinct_names_when_the_tool_set_is_made_then_the_tools_and_the_mail_tools_are_all_in_it()
{
    let set = ToolSet::new(
        vec![tool("search", Behavior::Echo)],
        vec![tool("send_mail", Behavior::Echo)],
    )
    .unwrap();

    assert_eq!(set.tools().len(), 2);
    assert!(set.find("send_mail").is_some() && set.find("nothing").is_none());
    assert_eq!(set.specs().len(), 2);
}
