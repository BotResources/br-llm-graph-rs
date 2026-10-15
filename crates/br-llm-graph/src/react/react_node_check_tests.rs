//! The build checks of the ReAct loop's nodes: the enabled key, the tool
//! concurrency key and the keys of the system sources.

use std::sync::Arc;

use br_llm_clients_core::Client;

use crate::graph::{Always, GraphBuilder, Limit, Target};
use crate::react::react_node_support::*;
use crate::react::source::Source;
use crate::react::test_support::*;
use crate::value::EndLabel;

#[test]
fn given_an_enabled_key_that_is_not_a_list_of_strings_when_built_then_refused() {
    let client: Arc<dyn Client> = Arc::new(mock(Vec::new()));
    for name in ["chat", "missing"] {
        let mut llm = llm_node(client.clone(), Vec::new());
        llm.tools = vec![Arc::new(EchoTool)];
        llm.enabled = Some(key(name));
        let built = GraphBuilder::new(schema())
            .entry(nid("llm"))
            .node(nid("llm"), llm)
            .edge(
                nid("llm"),
                Always(Target::End(EndLabel::new("done").unwrap())),
            )
            .build();
        assert!(matches!(
            crate::testkit::refusal(built),
            crate::error::GraphError::EnabledKeyMismatch { key: found } if found == key(name)
        ));
        let built = GraphBuilder::new(schema())
            .entry(nid("tools"))
            .node(
                nid("tools"),
                tool_node(vec![Arc::new(EchoTool)], None, Some(key(name))),
            )
            .edge(
                nid("tools"),
                Always(Target::End(EndLabel::new("done").unwrap())),
            )
            .build();
        assert!(matches!(
            crate::testkit::refusal(built),
            crate::error::GraphError::EnabledKeyMismatch { .. }
        ));
    }
}

#[test]
fn given_a_tool_concurrency_on_a_key_that_is_not_an_int_configuration_key_when_built_then_refused()
{
    for name in ["base", "missing"] {
        let tools = tool_node(
            vec![Arc::new(EchoTool)],
            Some(Limit::Config(key(name))),
            None,
        );
        assert!(matches!(
            built_alone(tools),
            crate::error::GraphError::LimitKeyMismatch { key: found } if found == key(name)
        ));
    }
}

#[test]
fn given_a_system_source_on_a_key_that_holds_no_text_when_built_then_refused() {
    let client: Arc<dyn Client> = Arc::new(mock(Vec::new()));
    let llm = |source: Source| llm_node(client.clone(), vec![Source::Config(key("base")), source]);
    for (source, name) in [
        (Source::State(key("chat")), "chat"),
        (Source::State(key("missing")), "missing"),
        (Source::State(key("base")), "base"),
        (Source::Config(key("log")), "log"),
        (Source::Config(key("missing")), "missing"),
    ] {
        assert!(matches!(
            built_alone(llm(source)),
            crate::error::GraphError::SourceKeyMismatch { key: found } if found == key(name)
        ));
    }
    let accepted = GraphBuilder::new(schema())
        .entry(nid("only"))
        .node(nid("only"), llm(Source::State(key("log"))))
        .edge(
            nid("only"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build();
    assert!(accepted.is_ok());
}
