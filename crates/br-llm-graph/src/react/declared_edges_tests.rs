//! The library declares the targets of every edge of the graphs it builds:
//! the ReAct agent, the generator-critic and the round-limit nodes.

use std::sync::Arc;

use crate::graph::{FnNode, Graph, NodeFuture, Target};
use crate::react::generator_critic_support::{cut_generator, duo, generator};
use crate::react::react_agent_support::{agent, limited, nid};
use crate::react::round_limit::OnLimit;
use crate::react::test_support::{key, mock};
use crate::react::test_tools::{EchoTool, TallyTool};
use crate::state::{Config, State};
use crate::value::EndLabel;

fn to(name: &str) -> Target {
    Target::Node(nid(name))
}

fn end(label: &str) -> Target {
    Target::End(EndLabel::new(label).unwrap())
}

/// Each node of `graph`, in registration order, with the targets its edge
/// declares; every edge must declare.
fn declared(graph: &Graph) -> Vec<(String, Vec<Target>)> {
    graph
        .order()
        .iter()
        .map(|id| {
            let targets = graph
                .edge(id)
                .and_then(|entry| entry.declared.as_deref())
                .unwrap_or_else(|| panic!("the edge of {id} is undeclared"))
                .to_vec();
            (id.as_str().to_owned(), targets)
        })
        .collect()
}

fn named(rows: Vec<(&str, Vec<Target>)>) -> Vec<(String, Vec<Target>)> {
    rows.into_iter()
        .map(|(id, targets)| (id.to_owned(), targets))
        .collect()
}

#[test]
fn given_a_react_agent_when_built_then_every_edge_declares_its_targets() {
    let graph = agent(&mock(Vec::new())).graph().unwrap();
    assert_eq!(
        declared(&graph),
        named(vec![
            ("enter", vec![to("llm"), to("answer")]),
            ("llm", vec![to("tools"), to("answer")]),
            ("tools", vec![to("llm")]),
            ("answer", vec![end("done")]),
        ])
    );
}

#[test]
fn given_a_react_agent_without_tools_when_built_then_its_tools_node_is_declared_to_answer_a_call_it_lacks()
 {
    let mut agent = agent(&mock(Vec::new()));
    agent.tools = Vec::new();
    let graph = agent.graph().unwrap();
    assert_eq!(
        declared(&graph),
        named(vec![
            ("enter", vec![to("llm"), to("answer")]),
            ("llm", vec![to("tools"), to("answer")]),
            ("tools", vec![to("llm")]),
            ("answer", vec![end("done")]),
        ])
    );
}

#[test]
fn given_a_split_of_the_tools_when_built_then_the_model_call_declares_each_tool_node_in_order() {
    let mut agent = agent(&mock(Vec::new()));
    agent.tools = vec![Arc::new(EchoTool), Arc::new(TallyTool)];
    agent.tool_nodes = vec![
        (nid("talliers"), vec![Arc::new(TallyTool) as _]),
        (nid("echoers"), vec![Arc::new(EchoTool) as _]),
    ];
    let graph = agent.graph().unwrap();
    assert_eq!(
        declared(&graph),
        named(vec![
            ("enter", vec![to("llm"), to("answer")]),
            ("llm", vec![to("talliers"), to("echoers"), to("answer")]),
            ("talliers", vec![to("llm")]),
            ("echoers", vec![to("llm")]),
            ("answer", vec![end("done")]),
        ])
    );
}

#[test]
fn given_a_round_limit_that_continues_when_built_then_its_node_goes_back_to_the_model_call() {
    let on_limit = OnLimit::Continue {
        node: nid("limit"),
        flag: key("limited"),
    };
    let graph = limited(&mock(Vec::new()), 2, on_limit).graph().unwrap();
    assert_eq!(
        declared(&graph),
        named(vec![
            ("enter", vec![to("llm"), to("answer")]),
            ("llm", vec![to("tools"), to("limit"), to("answer")]),
            ("tools", vec![to("llm")]),
            ("limit", vec![to("llm")]),
            ("answer", vec![end("done")]),
        ])
    );
}

#[test]
fn given_a_round_limit_that_ends_when_built_then_its_node_goes_to_the_answer() {
    let on_limit = OnLimit::End {
        node: nid("limit"),
        flag: key("limited"),
    };
    let graph = limited(&mock(Vec::new()), 2, on_limit).graph().unwrap();
    let rows = declared(&graph);
    assert!(rows.contains(&(
        "llm".to_owned(),
        vec![to("tools"), to("limit"), to("answer")]
    )));
    assert!(rows.contains(&("limit".to_owned(), vec![to("answer")])));
}

#[test]
fn given_a_round_limit_that_fails_when_built_then_no_limit_node_is_declared() {
    let graph = limited(&mock(Vec::new()), 2, OnLimit::Error)
        .graph()
        .unwrap();
    let rows = declared(&graph);
    assert!(rows.contains(&("llm".to_owned(), vec![to("tools"), to("answer")])));
    assert!(!graph.contains(&nid("limit")));
}

#[test]
fn given_a_generator_critic_when_built_then_every_edge_declares_its_targets() {
    let graph = duo(&mock(Vec::new()), &mock(Vec::new())).graph().unwrap();
    assert_eq!(
        declared(&graph),
        named(vec![
            ("generate", vec![to("tools"), to("check")]),
            ("tools", vec![to("generate")]),
            (
                "route",
                vec![to("generate"), to("check"), to("critique"), to("finish")]
            ),
            ("check", vec![to("generate"), to("critique"), to("finish")]),
            ("critique", vec![to("generate"), to("finish")]),
            ("finish", vec![end("validated"), end("exhausted")]),
        ])
    );
}

#[test]
fn given_a_generator_with_tools_and_a_limit_that_ends_when_built_then_the_loop_declares_its_targets()
 {
    let (generator_client, critic_client) = (mock(Vec::new()), mock(Vec::new()));
    let mut duo = duo(&generator_client, &critic_client);
    duo.generator = cut_generator(&generator_client);
    let rows = declared(&duo.graph().unwrap());
    assert!(rows.contains(&(
        "generate".to_owned(),
        vec![to("tools"), to("limit"), to("check")]
    )));
    assert!(rows.contains(&("tools".to_owned(), vec![to("generate")])));
    assert!(rows.contains(&("limit".to_owned(), vec![to("check")])));
}

#[test]
fn given_a_prepare_node_when_built_then_it_declares_the_routing() {
    let mut duo = duo(&mock(Vec::new()), &mock(Vec::new()));
    duo.generator = generator(&mock(Vec::new()));
    duo.prepare = Some(Box::new(FnNode::new(
        |_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> { Box::pin(async { Ok(Vec::new()) }) },
    )));
    let rows = declared(&duo.graph().unwrap());
    assert!(rows.contains(&("prepare".to_owned(), vec![to("route")])));
}
