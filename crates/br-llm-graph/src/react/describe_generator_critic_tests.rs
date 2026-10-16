//! A generator-critic's description: its own graph; each seat described like
//! a ReAct agent's model-calling node; the counts on the nodes that read
//! them.

use br_llm_clients_core::{Parameter, ParameterKind, Parameters, StructuredShape};
use serde_json::json;

use crate::describe::NodeDescription;
use crate::describe::test_support::{assert_written, json, node_json};
use crate::graph::{
    Always, Context, Count, GraphBuilder, Input, Node, NodeFuture, SubGraph, Target,
};
use crate::react::generator_critic_support::{duo, nid};
use crate::react::given::{History, Prompt, Reads};
use crate::react::rule_check::Ruling;
use crate::react::seats::CriticHistory;
use crate::react::test_support::{ckey, key, mock};
use crate::react::verdict::VerdictOverride;
use crate::state::{Config, Kind, Schema, State};
use crate::value::EndLabel;

#[test]
fn given_a_generator_critic_when_described_then_its_nodes_hold_its_seats_and_counts() {
    let graph = duo(&mock(Vec::new()), &mock(Vec::new())).graph().unwrap();
    let described = graph.describe();
    let kinds: Vec<(String, String)> = json(&described)["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| {
            (
                node["id"].as_str().unwrap().to_owned(),
                node["kind"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let expected = [
        ("generate", "model_call"),
        ("route", "node"),
        ("check", "node"),
        ("critique", "model_call"),
        ("finish", "node"),
    ];
    assert_eq!(
        kinds,
        expected
            .map(|(id, kind)| (id.to_owned(), kind.to_owned()))
            .to_vec()
    );
    let generate = node_json(&described, "generate");
    assert_eq!(generate["join"], true);
    assert_eq!(
        generate["author"],
        json!({"id": "writer", "name": "writer"})
    );
    assert_written(
        &node_json(&described, "check"),
        &json!({
            "id": "check", "kind": "node",
            "edge": {"targets": [{"node": "generate"}, {"node": "critique"}, {"node": "finish"}]},
            "revision_limit": {"fixed": 2},
            "empty_answer_critique": "ready_made"
        }),
    );
    assert_written(
        &node_json(&described, "critique"),
        &json!({
            "id": "critique", "kind": "model_call",
            "edge": {"targets": [{"node": "generate"}, {"node": "finish"}]},
            "author": {"id": "reviewer", "name": "reviewer"},
            "model": "fixed", "system": "fixed", "history": "ready_made",
            "max_critiques": {"fixed": 3},
            "final_generation": true,
            "verdict_retries": {"fixed": 0},
            "review_request": "ready_made",
            "verdict": "default"
        }),
    );
}

#[test]
fn given_its_options_given_each_way_when_described_then_each_says_how() {
    let mut duo = duo(&mock(Vec::new()), &mock(Vec::new()));
    duo.max_critiques = Count::Config(key("budget"));
    duo.revision_limit = Count::Config(key("revisions"));
    duo.verdict_retries = Count::Fixed(1);
    duo.final_generation = false;
    duo.rule_check = Some(crate::react::given::Given::from_fn(Reads::new(), |_| {
        Ok(Ruling::Pass)
    }));
    duo.empty_answer_critique = Some(Prompt::text("Say something."));
    duo.critic.review_request = Some(Prompt::from_config(&ckey("review")));
    duo.critic.history = CriticHistory::Given(History::from_fn(Reads::new(), |_| Ok(Vec::new())));
    let described = duo.graph().unwrap().describe();
    let check = node_json(&described, "check");
    assert_eq!(check["revision_limit"], json!({"config": "revisions"}));
    assert_eq!(check["rule_check"], "function");
    assert_eq!(check["empty_answer_critique"], "fixed");
    let critique = node_json(&described, "critique");
    assert_eq!(critique["max_critiques"], json!({"config": "budget"}));
    assert_eq!(critique["verdict_retries"], json!({"fixed": 1}));
    assert_eq!(critique["final_generation"], false);
    assert_eq!(critique["review_request"], "ready_made");
    assert_eq!(critique["history"], "function");
}

#[test]
fn given_the_critics_ready_made_histories_and_an_override_when_described_then_ready_made_and_override()
 {
    let mut briefed = duo(&mock(Vec::new()), &mock(Vec::new()));
    briefed.critic.history = CriticHistory::Brief(crate::react::test_support::skey("brief"));
    let critique = node_json(&briefed.graph().unwrap().describe(), "critique");
    assert_eq!(critique["history"], "ready_made");
    let mut duo = duo(&mock(Vec::new()), &mock(Vec::new()));
    duo.critic.history = CriticHistory::Latest;
    let params = Parameters::new(vec![
        Parameter::new(
            "is_valid",
            "a secret description",
            true,
            ParameterKind::Boolean,
        )
        .unwrap(),
        Parameter::new("message", "", true, ParameterKind::String).unwrap(),
    ])
    .unwrap();
    duo.critic.verdict = Some(VerdictOverride {
        shape: StructuredShape::new("review", None, params).unwrap(),
        reader: None,
    });
    let critique = node_json(&duo.graph().unwrap().describe(), "critique");
    assert_eq!(critique["history"], "ready_made");
    assert_eq!(critique["verdict"], "override");
}

/// A host's `prepare`, with its own kind label.
struct OpenDocument;

impl Node for OpenDocument {
    fn run<'a>(&'a self, _s: &'a State, _c: &'a Config, _x: &'a Context) -> NodeFuture<'a> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn describe(&self) -> NodeDescription {
        NodeDescription::labelled("loader")
            .unwrap()
            .realizing(["OpenDocument"])
    }
}

#[test]
fn given_a_prepare_node_when_described_then_the_hosts_description_is_forwarded() {
    let mut duo = duo(&mock(Vec::new()), &mock(Vec::new()));
    duo.prepare = Some(Box::new(OpenDocument));
    let described = duo.graph().unwrap().describe();
    assert_eq!(json(&described)["entry"], "prepare");
    assert_written(
        &node_json(&described, "prepare"),
        &json!({"id": "prepare", "kind": "loader", "realizes": ["OpenDocument"],
               "edge": {"targets": [{"node": "route"}]}}),
    );
}

#[test]
fn given_a_call_of_a_generator_critic_when_described_then_its_kind_is_generator_critic() {
    let graph = duo(&mock(Vec::new()), &mock(Vec::new())).graph().unwrap();
    let schema = Schema::builder()
        .state_by_name(key("question"), Kind::Conversation)
        .build();
    let call = SubGraph::call(graph).input(key("opening"), Input::From(key("question")));
    let parent = GraphBuilder::new(schema)
        .entry(nid("duo"))
        .subgraph(nid("duo"), call)
        .edge(
            nid("duo"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();
    let node = node_json(&parent.describe(), "duo");
    assert_eq!(node["kind"], "generator_critic");
    assert_eq!(node["graph"]["entry"], "route");
}
