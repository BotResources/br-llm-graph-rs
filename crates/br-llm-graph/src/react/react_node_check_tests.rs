//! The build checks of the ReAct loop's nodes: the enabled key, the tool
//! concurrency key and the keys its functions declare.

use std::sync::Arc;

use br_llm_clients_core::Client;

use crate::error::GraphError;
use crate::graph::{Always, GraphBuilder, Limit, Target};
use crate::react::given::Prompt;
use crate::react::react_agent::ReactAgent;
use crate::react::react_node_support::*;
use crate::react::test_support::*;
use crate::state::Kind;
use crate::value::EndLabel;

#[test]
fn given_an_enabled_key_that_is_not_a_list_of_strings_when_built_then_refused() {
    let client: Arc<dyn Client> = Arc::new(mock(Vec::new()));
    for name in ["chat", "missing"] {
        let mut llm = llm_node(client.clone(), Prompt::text(""));
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
fn given_a_function_reading_a_key_the_graph_lacks_or_holds_with_another_kind_when_built_then_refused()
 {
    let client: Arc<dyn Client> = Arc::new(mock(Vec::new()));
    let llm = |system: Prompt<ReactAgent>| llm_node(client.clone(), system);
    for (system, name) in [
        (Prompt::from_state(&skey("missing")), "missing"),
        (Prompt::from_state(&skey("base")), "base"),
        (Prompt::from_config(&ckey("log")), "log"),
        (Prompt::from_config(&ckey("missing")), "missing"),
    ] {
        assert!(matches!(
            built_alone(llm(system)),
            GraphError::UnknownKey { key: found } if found == key(name)
        ));
    }
    for (system, name, found) in [
        (Prompt::from_state(&skey("chat")), "chat", "conversation"),
        (Prompt::from_state(&skey("log")), "log", "list"),
    ] {
        assert!(matches!(
            built_alone(llm(system)),
            GraphError::KindMismatch { key: named, expected: Kind::Str, found: seen }
                if named == key(name) && seen == found
        ));
    }
    let accepted = GraphBuilder::new(schema())
        .entry(nid("only"))
        .node(nid("only"), llm(Prompt::from_state_list(&skey("log"))))
        .edge(
            nid("only"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build();
    assert!(accepted.is_ok());
}
