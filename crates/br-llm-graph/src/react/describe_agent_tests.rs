//! A ReAct agent's description: its own graph, its own nodes included; its
//! model-calling node holds its author, its tool names, its enabled key, its
//! tool concurrency and round limits, its reply retries, and how each
//! subject is given.

use std::sync::Arc;

use br_llm_clients_core::Client;
use serde_json::json;

use crate::describe::test_support::{json, node_json};
use crate::graph::{Always, GraphBuilder, Input, Limit, Output, SubGraph, Target};
use crate::react::given::{History, Model, Prompt, Reads};
use crate::react::react_agent_support::{agent, limited, nid};
use crate::react::react_durable_support::durable;
use crate::react::round_limit::OnLimit;
use crate::react::test_support::{key, mock};
use crate::react::test_tools::{EchoTool, TallyTool};
use crate::state::{Kind, Schema};
use crate::testkit::TestStore;
use crate::value::EndLabel;

#[test]
fn given_a_react_agent_when_described_then_its_own_graph_and_nodes_are_written() {
    let graph = agent(&mock(Vec::new())).graph().unwrap();
    assert_eq!(
        json(&graph.describe()),
        json!({
            "description_version": "br-llm-graph-description/1",
            "entry": "enter",
            "signature": {
                "inputs": {"opening": {"type": "conversation"}},
                "outputs": {"answer": {"type": "str"}}
            },
            "schema": {
                "state": {"answer": {"type": "str"}, "opening": {"type": "conversation"}},
                "config": {"base": {"type": "str"}}
            },
            "nodes": [
                {"id": "enter", "kind": "node",
                 "edge": {"targets": [{"node": "llm"}, {"node": "answer"}]}},
                {"id": "llm", "kind": "model_call", "join": true,
                 "edge": {"targets": [{"node": "tools"}, {"node": "answer"}]},
                 "author": {"id": "agent"},
                 "model": "fixed", "system": "ready_made", "history": "ready_made",
                 "tools": ["echo"],
                 "reply_retries": {"fixed": 0}, "reminder": "ready_made"},
                {"id": "tools", "kind": "tools", "edge": {"targets": [{"node": "llm"}]},
                 "tools": ["echo"]},
                {"id": "answer", "kind": "node", "edge": {"targets": [{"end": "done"}]}}
            ]
        })
    );
}

#[test]
fn given_a_durable_agent_when_described_then_its_thread_is_in_its_signature_and_no_private_key_is()
{
    let store = Arc::new(TestStore::new());
    let described = durable(&mock(Vec::new()), &store)
        .graph()
        .unwrap()
        .describe();
    let json = json(&described);
    assert_eq!(
        json["signature"]["inputs"]["thread"],
        json!({"type": "str"})
    );
    assert_eq!(
        json["signature"]["outputs"]["thread"],
        json!({"type": "str"})
    );
    let state = json["schema"]["state"].as_object().unwrap();
    for private in ["conversation", "turn_rank", "call_turns", "failed_turns"] {
        assert!(!state.contains_key(private), "{private}");
    }
}

#[test]
fn given_subjects_given_each_way_when_described_then_each_says_how_never_what() {
    let mut reads = Reads::new();
    let topic = reads.state::<String>("topic").unwrap();
    let mut agent = agent(&mock(Vec::new()));
    let client: Arc<dyn Client> = Arc::new(mock(Vec::new()));
    agent.model = Model::schedule(client.clone(), vec![client]);
    agent.system = Prompt::from_fn(reads, move |ctx| {
        Ok(format!("Research {}.", ctx.get(&topic)?))
    });
    agent.history = Some(History::value(Vec::new()));
    let llm = node_json(&agent.graph().unwrap().describe(), "llm");
    assert_eq!(
        (
            llm["model"].clone(),
            llm["system"].clone(),
            llm["history"].clone()
        ),
        (json!("ready_made"), json!("function"), json!("fixed"))
    );
    let mut agent = crate::react::react_agent_support::agent(&mock(Vec::new()));
    agent.system = Prompt::text("Research.");
    agent.history = Some(History::from_fn(Reads::new(), |_| Ok(Vec::new())));
    let llm = node_json(&agent.graph().unwrap().describe(), "llm");
    assert_eq!(
        (llm["system"].clone(), llm["history"].clone()),
        (json!("fixed"), json!("function"))
    );
}

#[test]
fn given_reply_retries_and_a_reminder_when_described_then_the_model_call_holds_the_count_and_how_the_reminder_is_given()
 {
    let mut agent = agent(&mock(Vec::new()));
    agent.reply_retries = Some(crate::graph::Count::Config(key("retries")));
    agent.reminder = Some(Prompt::text("Your JSON was wrong: {reason}."));
    let described = agent.graph().unwrap().describe();
    let llm = node_json(&described, "llm");
    assert_eq!(llm["reply_retries"], json!({"config": "retries"}));
    assert_eq!(llm["reminder"], "fixed");
    assert_eq!(
        json(&described)["schema"]["config"]["retries"],
        json!({"type": "int"})
    );
    let mut agent = crate::react::react_agent_support::agent(&mock(Vec::new()));
    agent.reply_retries = Some(crate::graph::Count::Fixed(2));
    agent.reminder = Some(Prompt::from_fn(Reads::new(), |_| Ok("{reason}".to_owned())));
    let llm = node_json(&agent.graph().unwrap().describe(), "llm");
    assert_eq!(
        (llm["reply_retries"].clone(), llm["reminder"].clone()),
        (json!({"fixed": 2}), json!("function"))
    );
}

#[test]
fn given_an_enabled_key_and_limits_from_configuration_when_described_then_the_model_call_holds_them()
 {
    let mut agent = limited(
        &mock(Vec::new()),
        1,
        OnLimit::Continue {
            node: nid("limit"),
            flag: key("limited"),
        },
    );
    agent.enabled = Some(key("enabled"));
    agent.tool_concurrency = Some(Limit::Config(key("width")));
    let described = agent.graph().unwrap().describe();
    let llm = node_json(&described, "llm");
    assert_eq!(llm["enabled"], "enabled");
    assert_eq!(llm["tool_concurrency"], json!({"config": "width"}));
    assert_eq!(
        llm["round_limit"],
        json!({"max_rounds": {"fixed": 1}, "on_limit": {"continue": {"node": "limit", "flag": "limited"}}})
    );
    assert_eq!(
        node_json(&described, "limit"),
        json!({"id": "limit", "kind": "node", "edge": {"targets": [{"node": "llm"}]}})
    );
}

#[test]
fn given_round_limits_that_end_or_fail_when_described_then_their_policy_is_written() {
    let ending = OnLimit::End {
        node: nid("limit"),
        flag: key("limited"),
    };
    let llm = node_json(
        &limited(&mock(Vec::new()), 2, ending)
            .graph()
            .unwrap()
            .describe(),
        "llm",
    );
    assert_eq!(
        llm["round_limit"]["on_limit"],
        json!({"end": {"node": "limit", "flag": "limited"}})
    );
    let mut failing = limited(&mock(Vec::new()), 2, OnLimit::Error);
    failing.round_limit.as_mut().unwrap().max_rounds = Limit::Config(key("rounds"));
    let llm = node_json(&failing.graph().unwrap().describe(), "llm");
    assert_eq!(
        llm["round_limit"],
        json!({"max_rounds": {"config": "rounds"}, "on_limit": "error"})
    );
}

#[test]
fn given_a_split_of_the_tools_when_described_then_each_tool_node_names_its_own_tools() {
    let mut agent = agent(&mock(Vec::new()));
    agent.tools = vec![Arc::new(EchoTool), Arc::new(TallyTool)];
    agent.tool_nodes = vec![
        (nid("talliers"), vec![Arc::new(TallyTool) as _]),
        (nid("echoers"), vec![Arc::new(EchoTool) as _]),
    ];
    let described = agent.graph().unwrap().describe();
    assert_eq!(
        node_json(&described, "llm")["tools"],
        json!(["echo", "tally"])
    );
    assert_eq!(node_json(&described, "talliers")["tools"], json!(["tally"]));
    assert_eq!(node_json(&described, "echoers")["kind"], "tools");
}

#[test]
fn given_an_agent_without_tools_when_described_then_its_model_call_has_no_tools_field() {
    let mut agent = agent(&mock(Vec::new()));
    agent.tools = Vec::new();
    let llm = node_json(&agent.graph().unwrap().describe(), "llm");
    assert!(llm.get("tools").is_none());
    assert!(llm.get("enabled").is_none() && llm.get("round_limit").is_none());
}

#[test]
fn given_a_call_of_a_react_agent_when_described_then_its_kind_is_react_agent_holding_its_graph() {
    let agent_graph = agent(&mock(Vec::new())).graph().unwrap();
    let schema = Schema::builder()
        .state_by_name(key("question"), Kind::Conversation)
        .state_by_name(key("report"), Kind::Str)
        .config_by_name(key("base"), Kind::Str)
        .build();
    let call = SubGraph::call(agent_graph.clone())
        .input(key("opening"), Input::From(key("question")))
        .config(key("base"), Input::Config(key("base")))
        .output(key("answer"), Output::Set(key("report")));
    let graph = GraphBuilder::new(schema)
        .entry(nid("researcher"))
        .subgraph(nid("researcher"), call)
        .edge(
            nid("researcher"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();
    let node = node_json(&graph.describe(), "researcher");
    assert_eq!(node["kind"], "react_agent");
    assert_eq!(
        node["inputs"],
        json!([{"child": "opening", "source": {"from": "question"}}])
    );
    let mut own = json(&agent_graph.describe());
    own.as_object_mut()
        .unwrap()
        .shift_remove("description_version");
    assert_eq!(node["graph"], own);
}
