use br_agentic_nodes::{GET_MAIL_FILES, SEND_MAIL};
use br_llm_clients_core::{ParameterKind, ToolSpec};

use crate::common::*;
use crate::kit::*;

fn parameter<'a>(spec: &'a ToolSpec, name: &str) -> &'a br_llm_clients_core::Parameter {
    spec.parameters()
        .iter()
        .find(|parameter| parameter.name() == name)
        .unwrap_or_else(|| panic!("no parameter {name}"))
}

fn is_string_array(kind: &ParameterKind) -> bool {
    matches!(kind, ParameterKind::Array { items } if matches!(**items, ParameterKind::String))
}

#[tokio::test(start_paused = true)]
async fn given_a_node_with_an_inbox_when_it_calls_the_model_then_every_request_offers_both_mail_tools_and_the_tools_of_the_port()
 {
    let (noop, _) = echo("noop");
    let rig = Rig::new().with_tools(vec![noop]);
    let model = Scripted::new(vec![Reply::call("noop"), Reply::text("done")]);
    let node = plain(&rig, &model, "Agent");

    Driver::result(&node).await;

    for request in model.requests() {
        let offered: Vec<&str> = request
            .tools()
            .iter()
            .map(|spec| spec.name().as_str())
            .collect();
        assert!(offered.contains(&SEND_MAIL), "{offered:?}");
        assert!(offered.contains(&GET_MAIL_FILES), "{offered:?}");
        assert!(offered.contains(&"noop"), "{offered:?}");
        assert_eq!(offered.len(), 3);
    }
    assert_eq!(SEND_MAIL, "send_mail");
    assert_eq!(GET_MAIL_FILES, "get_mail_files");
}

#[tokio::test(start_paused = true)]
async fn given_a_node_without_an_inbox_when_it_calls_the_model_then_no_mail_tool_is_offered() {
    let (noop, _) = echo("noop");
    let rig = Rig::new().with_tools(vec![noop]);
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let request = &model.requests()[0];
    let offered: Vec<&str> = request
        .tools()
        .iter()
        .map(|spec| spec.name().as_str())
        .collect();
    assert_eq!(offered, vec!["noop"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_node_with_an_inbox_when_the_mail_tools_are_offered_then_their_specs_are_the_fixed_ones()
 {
    let rig = Rig::new();
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = plain(&rig, &model, "Agent");

    Driver::result(&node).await;

    let request = &model.requests()[0];
    let spec_of = |name: &str| -> &ToolSpec {
        request
            .tools()
            .iter()
            .find(|spec| spec.name().as_str() == name)
            .unwrap()
    };
    let send = spec_of(SEND_MAIL);
    assert_eq!(send.parameters().len(), 5);
    for (name, required) in [
        ("to", true),
        ("cc", false),
        ("subject", true),
        ("body", true),
        ("attachments", false),
    ] {
        assert_eq!(parameter(send, name).is_required(), required, "{name}");
    }
    assert!(is_string_array(parameter(send, "to").kind()));
    assert!(is_string_array(parameter(send, "cc").kind()));
    assert!(matches!(
        parameter(send, "subject").kind(),
        ParameterKind::String
    ));
    assert!(matches!(
        parameter(send, "body").kind(),
        ParameterKind::String
    ));
    let ParameterKind::Array { items } = parameter(send, "attachments").kind() else {
        panic!("attachments is not an array")
    };
    let ParameterKind::Object { parameters } = &**items else {
        panic!("an attachment is not an object")
    };
    let names: Vec<(&str, bool)> = parameters
        .iter()
        .map(|p| (p.name(), p.is_required()))
        .collect();
    assert_eq!(names, vec![("name", true), ("document", true)]);
    let files = spec_of(GET_MAIL_FILES);
    assert_eq!(files.parameters().len(), 1);
    assert!(parameter(files, "mail_id").is_required());
    assert!(matches!(
        parameter(files, "mail_id").kind(),
        ParameterKind::String
    ));
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_of_the_port_named_like_a_mail_tool_when_the_node_has_an_inbox_then_the_instance_fails()
 {
    for name in [SEND_MAIL, GET_MAIL_FILES] {
        let (clash, _) = echo(name);
        let rig = Rig::new().with_tools(vec![clash]);
        let model = Scripted::new(vec![Reply::text("done")]);
        let node = plain(&rig, &model, "Agent");

        let cause = Driver::failure(&node).await;

        assert!(!cause.is_empty());
        assert_eq!(model.calls(), 0, "{name}");
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_of_the_port_named_like_a_mail_tool_when_the_node_has_no_inbox_then_the_name_is_free()
 {
    let (clash, _) = echo(SEND_MAIL);
    let rig = Rig::new().with_tools(vec![clash]);
    let model = Scripted::new(vec![Reply::call(SEND_MAIL), Reply::text("done")]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    let results = outcomes(&rig.conversation());
    assert!(!results[0].is_error);
}
