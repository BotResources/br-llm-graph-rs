//! Two declared tools with one name: a request no client accepts, so the
//! ReAct loop refuses them at build, before its rules on tool nodes, for a
//! `ReactAgent` and for a generator seat, whatever the split between tool
//! nodes.

use std::sync::Arc;

use br_llm_clients_core::BuildError;
use br_llm_messages::ToolName;

use crate::error::GraphError;
use crate::graph::{Count, Graph};
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::{critic, generator};
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::{agent, nid};
use crate::react::seats::GeneratorSeat;
use crate::react::test_support::{EchoTool, TallyTool, mock};
use crate::react::tool::Tool;
use crate::value::NodeId;

fn echo() -> Arc<dyn Tool> {
    Arc::new(EchoTool)
}

fn tally() -> Arc<dyn Tool> {
    Arc::new(TallyTool)
}

/// A tool node of a split: its id and the tools it runs.
type ToolNodeSplit = (NodeId, Vec<Arc<dyn Tool>>);

/// Declared tools and their split between tool nodes.
type Shape = (Vec<Arc<dyn Tool>>, Vec<ToolNodeSplit>);

/// The declared tools and their split: two `echo` without a split, two
/// `echo` run by two nodes, two `echo` covered by name by one node, and two
/// `echo` beside a `tally` no node runs.
fn shapes() -> Vec<Shape> {
    vec![
        (vec![echo(), echo()], Vec::new()),
        (
            vec![echo(), echo()],
            vec![(nid("first"), vec![echo()]), (nid("second"), vec![echo()])],
        ),
        (vec![echo(), echo()], vec![(nid("only"), vec![echo()])]),
        (
            vec![echo(), echo(), tally()],
            vec![(nid("only"), vec![echo()])],
        ),
    ]
}

/// The build refused the model-call step `node` with `DuplicateToolName`.
fn refused_as_duplicate(built: Result<Arc<Graph>, GraphError>, node: &str) {
    match built {
        Err(GraphError::InvalidNode {
            node: found,
            source,
        }) => {
            assert_eq!(found, nid(node));
            assert!(
                matches!(
                    *source,
                    GraphError::Request(BuildError::DuplicateToolName { ref name })
                        if *name == ToolName::new("echo").unwrap()
                ),
                "{source}"
            );
        }
        Err(other) => panic!("expected the duplicate to be refused, got: {other}"),
        Ok(_) => panic!("expected the duplicate to be refused, the graph was built"),
    }
}

#[test]
fn given_an_agent_declaring_two_tools_with_one_name_when_built_then_invalid_node_with_request() {
    for (tools, tool_nodes) in shapes() {
        let built = ReactAgent {
            tools,
            tool_nodes,
            ..agent(&mock(Vec::new()))
        }
        .graph();
        refused_as_duplicate(built, "llm");
    }
}

#[test]
fn given_a_generator_declaring_two_tools_with_one_name_when_built_then_invalid_node_with_request() {
    for (tools, tool_nodes) in shapes() {
        let built = GeneratorCritic::new(
            GeneratorSeat {
                tools,
                tool_nodes,
                ..generator(&mock(Vec::new()))
            },
            critic(&mock(Vec::new())),
            Count::Fixed(1),
        )
        .graph();
        refused_as_duplicate(built, "generate");
    }
}

#[test]
fn given_tools_with_distinct_names_when_built_then_the_rules_on_tool_nodes_apply() {
    // Positive controls: distinct names build, and a tool no node runs is
    // refused by the coverage rule.
    let split = || vec![(nid("only"), vec![echo()])];
    assert!(
        ReactAgent {
            tools: vec![echo(), tally()],
            ..agent(&mock(Vec::new()))
        }
        .graph()
        .is_ok()
    );
    assert!(matches!(
        ReactAgent {
            tools: vec![echo(), tally()],
            tool_nodes: split(),
            ..agent(&mock(Vec::new()))
        }
        .graph(),
        Err(GraphError::ToolNotCovered { .. })
    ));
}
