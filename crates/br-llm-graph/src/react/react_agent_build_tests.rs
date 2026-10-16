//! The build of a ReAct agent: its signature, its configuration keys, its
//! round limit's flag and the node ids it refuses.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::GraphError;
use crate::graph::Limit;
use crate::react::given::Prompt;
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::round_limit::OnLimit;
use crate::react::test_support::*;
use crate::state::Kind;

#[test]
fn given_an_agent_when_built_then_its_signature_names_opening_answer_and_the_limit_flag() {
    let client = mock(Vec::new());
    let plain = agent(&client).graph().unwrap();
    let signature = plain.signature();
    assert_eq!(
        signature.inputs,
        BTreeMap::from([(key("opening"), Kind::Conversation)])
    );
    assert_eq!(
        signature.outputs,
        BTreeMap::from([(key("answer"), Kind::Str)])
    );
    assert_eq!(
        plain.schema().config,
        BTreeMap::from([(key("base"), Kind::Str)])
    );

    let flagged = ReactAgent {
        system: Prompt::from_state(&skey("persona")),
        tool_concurrency: Some(Limit::Config(key("width"))),
        ..limited(
            &client,
            2,
            OnLimit::End {
                node: nid("limit"),
                flag: key("limit_reached"),
            },
        )
    }
    .graph()
    .unwrap();
    assert_eq!(
        flagged.signature().inputs,
        BTreeMap::from([
            (key("opening"), Kind::Conversation),
            (key("persona"), Kind::Str),
        ])
    );
    assert_eq!(
        flagged.signature().outputs.get(&key("limit_reached")),
        Some(&Kind::Bool)
    );
    assert_eq!(
        flagged.schema().config,
        BTreeMap::from([(key("width"), Kind::Int)])
    );
}

#[test]
fn given_a_limit_flag_that_names_the_answer_when_built_then_flag_key_mismatch() {
    let client = mock(Vec::new());
    let built = limited(
        &client,
        1,
        OnLimit::Continue {
            node: nid("limit"),
            flag: key("answer"),
        },
    )
    .graph();
    assert!(matches!(built, Err(GraphError::FlagKeyMismatch { .. })));
}

#[test]
fn given_a_limit_flag_that_names_another_key_of_the_graph_when_built_then_key_collision() {
    let client = mock(Vec::new());
    let on_limit = || OnLimit::End {
        node: nid("limit"),
        flag: key("topic"),
    };
    let read = ReactAgent {
        system: Prompt::from_state(&skey("topic")),
        ..limited(&client, 1, on_limit())
    };
    let input = ReactAgent {
        inputs: vec![(key("topic"), Kind::Bool)],
        ..limited(&client, 1, on_limit())
    };
    for agent in [read, input] {
        assert!(matches!(
            agent.graph(),
            Err(GraphError::KeyCollision { ref key }) if *key == self::key("topic")
        ));
    }
}

#[test]
fn given_a_tool_node_or_a_limit_node_named_like_a_node_of_the_agent_when_built_then_duplicate_node()
{
    let client = mock(Vec::new());
    for reserved in ["enter", "llm", "answer"] {
        let split = ReactAgent {
            tool_nodes: vec![(nid(reserved), vec![Arc::new(EchoTool) as _])],
            ..agent(&client)
        };
        let limit = limited(
            &client,
            1,
            OnLimit::End {
                node: nid(reserved),
                flag: key("limited"),
            },
        );
        for agent in [split, limit] {
            assert!(matches!(
                agent.graph(),
                Err(GraphError::DuplicateNode { ref id }) if *id == nid(reserved)
            ));
        }
    }
    let limit_on_tools = limited(
        &client,
        1,
        OnLimit::End {
            node: nid("tools"),
            flag: key("limited"),
        },
    );
    assert!(matches!(
        limit_on_tools.graph(),
        Err(GraphError::DuplicateNode { ref id }) if *id == nid("tools")
    ));
}
