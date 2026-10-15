//! The ids and keys a ReAct agent and a generator-critic hold whatever their
//! tools: without a split, `tools` is the graph's own node even when no tool
//! is declared; and an agent's flag naming `history` or `answer` is refused
//! before the rules of the ReAct loop.

use std::sync::Arc;

use crate::error::GraphError;
use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::{critic, generator, limit};
use crate::react::react_agent_support::{limited, nid};
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::seats::GeneratorSeat;
use crate::react::test_support::{EchoTool, key, mock};

fn end_on(node: &str) -> OnLimit {
    OnLimit::End {
        node: nid(node),
        flag: key("limited"),
    }
}

#[test]
fn given_an_agent_without_tools_when_its_limit_node_is_named_tools_then_duplicate_node() {
    let client = mock(Vec::new());
    for tools in [Vec::new(), vec![Arc::new(EchoTool) as _]] {
        let agent = crate::react::react_agent::ReactAgent {
            tools,
            ..limited(&client, 1, end_on("tools"))
        };
        assert!(matches!(
            agent.graph(),
            Err(GraphError::DuplicateNode { ref id }) if *id == nid("tools")
        ));
    }
    // Positive control: another id is free.
    let agent = crate::react::react_agent::ReactAgent {
        tools: Vec::new(),
        ..limited(&client, 1, end_on("limit"))
    };
    assert!(agent.graph().is_ok());
}

#[test]
fn given_a_generator_without_tools_when_its_limit_node_is_named_tools_then_duplicate_node() {
    let built = |node: &str| {
        GeneratorCritic::new(
            GeneratorSeat {
                round_limit: Some(RoundLimit {
                    max_rounds: limit(1),
                    on_limit: end_on(node),
                }),
                ..generator(&mock(Vec::new()))
            },
            critic(&mock(Vec::new())),
            Count::Fixed(1),
        )
        .graph()
    };
    assert!(matches!(
        built("tools"),
        Err(GraphError::DuplicateNode { ref id }) if *id == nid("tools")
    ));
    assert!(built("limit").is_ok(), "positive control");
}

#[test]
fn given_an_agent_flag_naming_the_answer_when_built_then_flag_key_mismatch_before_the_loop_rules() {
    // Two tools of one name would be refused by the ReAct loop.
    let client = mock(Vec::new());
    for flag in ["answer", "history"] {
        let mut agent = limited(
            &client,
            1,
            OnLimit::End {
                node: nid("limit"),
                flag: key(flag),
            },
        );
        agent.tools.push(Arc::new(EchoTool));
        assert!(
            matches!(
                agent.graph(),
                Err(GraphError::FlagKeyMismatch { ref key }) if *key == self::key(flag)
            ),
            "{flag}"
        );
    }
}
