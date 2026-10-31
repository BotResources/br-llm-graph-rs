use std::sync::Arc;

use br_agentic_contract::{AgenticNode, Site, WebAccess};
use br_agentic_nodes::{AuthorSource, REACT_AGENT, Settings};
use br_graph_contract::Node;

use crate::common::*;
use crate::kit::*;

#[tokio::test(start_paused = true)]
async fn given_a_run_when_the_turn_is_created_then_it_has_the_turn_id_of_the_requests_the_agent_of_the_agent_port_and_the_build_author()
 {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]);
    let model = Model::new(vec![Act::call("alpha"), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    let conversation = rig.conversation();
    let turns = ai_turns(&conversation);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].id(), model.requests()[0].reply_turn());
    assert_eq!(turns[0].agent(), rig.agent);
    assert_eq!(turns[0].author().as_str(), "Agent");
}

#[tokio::test(start_paused = true)]
async fn given_an_author_port_when_the_instance_runs_then_it_is_called_once_with_the_label_and_its_name_is_the_author()
 {
    let (tool, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![tool]);
    let model = Model::new(vec![
        Act::call("alpha"),
        Act::call("alpha"),
        Act::text("done"),
    ]);
    let author = SnapAuthor::new("Ada Lovelace");
    let capture = Capture::new(&rig.record);
    let node = node_with_ports(
        &rig,
        rig.prompts.clone(),
        Arc::new(SnapTools::new(&rig.record, rig.tools.clone()).into_port()),
        AuthorSource::Port(author.clone()),
        capture,
        plains(&[&model]),
    );

    Driver::result(&node).await;

    assert_eq!(
        author.seen.lock().unwrap().clone(),
        vec![("start".to_owned(), "agent".to_owned())]
    );
    let conversation = rig.conversation();
    assert_eq!(ai_turns(&conversation)[0].author().as_str(), "Ada Lovelace");
}

#[tokio::test(start_paused = true)]
async fn given_tools_when_each_step_is_called_then_the_request_offers_exactly_the_tools_of_the_port()
 {
    let (alpha, _) = echo("alpha");
    let (beta, _) = echo("beta");
    let rig = Rig::new().with_tools(vec![alpha, beta]);
    let model = Model::new(vec![
        Act::call("alpha"),
        Act::call("beta"),
        Act::text("done"),
    ]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    for request in model.requests() {
        let mut offered: Vec<_> = request
            .tools()
            .iter()
            .map(|spec| spec.name().as_str().to_owned())
            .collect();
        offered.sort();
        assert_eq!(offered, vec!["alpha", "beta"]);
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_node_without_inbox_when_the_step_is_called_then_no_mail_tool_is_offered() {
    let (alpha, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![alpha]);
    let model = Model::new(vec![Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    let offered: Vec<_> = model.requests()[0]
        .tools()
        .iter()
        .map(|spec| spec.name().as_str().to_owned())
        .collect();
    assert_eq!(offered, vec!["alpha"]);
}

#[tokio::test(start_paused = true)]
async fn given_tools_when_the_agent_runs_without_calling_them_then_the_thread_holds_no_trace_of_them()
 {
    let (alpha, _) = echo("alpha");
    let rig = Rig::new().with_tools(vec![alpha]);
    let model = Model::new(vec![Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    assert_eq!(model.requests()[0].tools().len(), 1);
    let held = format!("{:?}", rig.conversation());
    assert!(!held.contains("alpha"), "{held}");
    assert!(!held.contains("a test tool"), "{held}");
    assert!(!history_text(&model.requests()[0]).contains("a test tool"));
}

#[tokio::test(start_paused = true)]
async fn given_a_prompt_when_the_step_is_called_then_the_request_carries_the_system_prompt_of_the_agent_label()
 {
    let rig = Rig::new();
    let model = Model::new(vec![Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    let request = &model.requests()[0];
    assert_eq!(
        request.system().map(|text| text.as_str()),
        Some("system prompt of the agent")
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_react_agent_when_the_node_is_asked_its_metadata_then_it_is_agentic_react_agent_without_inbox()
 {
    let rig = Rig::new();
    let model = Model::new(vec![]);
    let node = node(&rig, plains(&[&model]));

    assert_eq!(node.kind(), "agentic");
    assert_eq!(node.subkind(), "react_agent");
    assert_eq!(REACT_AGENT, "react_agent");
    assert_eq!(node.name(), "agent");
    assert!(!node.has_inbox());
}

#[tokio::test(start_paused = true)]
async fn given_a_react_agent_with_an_inbox_when_the_node_is_asked_its_metadata_then_it_has_an_inbox()
 {
    let rig = Rig::new().with_inbox();
    let model = Model::new(vec![]);
    let node = node(&rig, plains(&[&model]));

    assert!(node.has_inbox());
    assert_eq!(node.kind(), "agentic");
    assert_eq!(node.subkind(), "react_agent");
}

#[tokio::test(start_paused = true)]
async fn given_a_web_access_value_when_the_settings_are_built_then_the_node_holds_it_and_runs() {
    let mut rig = Rig::new();
    let access = WebAccess::Whitelist(vec![Site::new("example.org").unwrap()]);
    rig.settings = Settings::new(
        rig.settings.step_limit,
        rig.settings.timeouts,
        access.clone(),
        rig.settings.file_system,
    );
    let model = Model::new(vec![Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    let update = Driver::result(&node).await;

    assert_eq!(rig.settings.web_access, access);
    assert_eq!(rig.settings.web_access.sites().len(), 1);
    assert_eq!(update.text, "done");
}
