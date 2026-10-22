//! The description of a graph holding a plain node, a map whose body calls
//! a graph of one LLM call, and a call of a ReAct agent, as JSON.

use std::num::NonZeroUsize;
use std::sync::{Arc, LazyLock};

use br_llm_clients_core::{OutputMode, ToolSpec};
use serde_json::{Value as Json, json};

use crate::describe::test_support::{assert_written, json};
use crate::graph::{
    Always, Count, FnEdge, FnNode, Graph, GraphBuilder, Input, ItemFailure, Limit, Map, Node,
    NodeFuture, Output, SubGraph, Target,
};
use crate::react::given::{Model, Prompt};
use crate::react::llm_call::{LlmCall, LlmOutput, LlmRequest};
use crate::react::react_agent::ReactAgent;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::test_support::{key, mock};
use crate::react::test_tools::spec;
use crate::react::tool::{Tool, ToolFuture};
use crate::state::{Config, Schema, State, StateKey};
use crate::value::{EndLabel, NodeId};

/// A tool of the researcher, named `name`; never called here.
struct Named(&'static LazyLock<ToolSpec>);

impl Tool for Named {
    fn spec(&self) -> &ToolSpec {
        self.0
    }

    fn call<'a>(
        &'a self,
        _arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        unreachable!("a description calls no tool")
    }
}

static SEARCH: LazyLock<ToolSpec> = LazyLock::new(|| spec("search"));
static FETCH: LazyLock<ToolSpec> = LazyLock::new(|| spec("fetch"));

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn done(label: &str) -> Target {
    Target::End(EndLabel::new(label).unwrap())
}

fn researcher() -> Arc<Graph> {
    let client = Arc::new(mock(Vec::new()));
    ReactAgent {
        author: br_llm_messages::Author::new("researcher", "Researcher").unwrap(),
        model: Model::client(client),
        system: Prompt::text("Research the question."),
        history: None,
        output_mode: OutputMode::Plain,
        tools: vec![Arc::new(Named(&SEARCH)), Arc::new(Named(&FETCH))],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: Some(RoundLimit {
            max_rounds: Limit::Fixed(NonZeroUsize::new(4).unwrap()),
            on_limit: OnLimit::Error,
        }),
        enabled: None,
        reply_retries: None,
        reminder: None,
        inputs: Vec::new(),
        thread_store: None,
    }
    .graph()
    .unwrap()
}

fn summarize() -> Arc<Graph> {
    let mut schema = Schema::builder();
    let topic: StateKey<String> = schema.state("topic").unwrap();
    let note: StateKey<String> = schema.state("note").unwrap();
    let call = LlmCall {
        model: Model::client(Arc::new(mock(Vec::new()))),
        system: Prompt::text("Summarize in one line."),
        request: LlmRequest::Text(Prompt::from_state(&topic)),
        output_mode: OutputMode::Plain,
        output: LlmOutput::Set(note.clone()),
        reply_retries: Some(Count::Fixed(2)),
        reminder: Some(Prompt::text("{reason}: write the note again.")),
    };
    Arc::new(
        GraphBuilder::new(schema.build())
            .entry(id("summarize"))
            .node(id("summarize"), call)
            .edge(id("summarize"), Always(done("done")))
            .input(&topic)
            .output(&note)
            .build()
            .unwrap(),
    )
}

fn graph() -> Graph {
    let mut schema = Schema::builder();
    let topics: StateKey<Vec<String>> = schema.state("topics").unwrap();
    let topic: StateKey<String> = schema.state("topic").unwrap();
    let notes: StateKey<Vec<String>> = schema.state("notes").unwrap();
    let question = schema
        .state::<br_llm_messages::Conversation>("question")
        .unwrap();
    let report: StateKey<String> = schema.state("report").unwrap();
    let width = schema.config::<i64>("width").unwrap();
    let plan = FnNode::new(|_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
        Box::pin(async { Ok(Vec::new()) })
    });
    let body = SubGraph::call(summarize())
        .input(key("topic"), Input::From(key("topic")))
        .output(key("note"), Output::Append(key("notes")));
    let summaries = Map::new(&topics, &topic, body)
        .max_concurrency(Limit::config(&width))
        .on_item_failure(ItemFailure::FailFast);
    let after = FnEdge::new(|_s: &State, _c: &Config| Ok(vec![Target::Node(id("researcher"))]))
        .targets([Target::Node(id("researcher")), done("skipped")]);
    let call = SubGraph::call(researcher())
        .input(key("opening"), Input::From(key("question")))
        .output(key("answer"), Output::Set(key("report")));
    GraphBuilder::new(schema.build())
        .entry(id("plan"))
        .node(id("plan"), plan.realizing(["PlanTopics"]))
        .map(id("summaries"), summaries)
        .node(id("researcher"), call.realizing(["Researcher"]))
        .edge(id("plan"), Always(Target::Node(id("summaries"))))
        .edge(id("summaries"), after)
        .edge(id("researcher"), Always(done("done")))
        .input(&question)
        .input(&topics)
        .output(&notes)
        .output(&report)
        .build()
        .unwrap()
}

#[test]
fn given_a_node_a_map_a_call_and_an_agent_when_described_then_the_json_is_the_published_shape() {
    let list = json!({"type": "list", "element": {"type": "str"}});
    assert_written(
        &json(&graph().describe()),
        &json!({
            "description_version": "br-llm-graph-description/1",
            "entry": "plan",
            "signature": {
                "inputs": {"question": {"type": "conversation"}, "topics": list},
                "outputs": {"notes": list, "report": {"type": "str"}}
            },
            "schema": {
                "state": {
                    "notes": list,
                    "question": {"type": "conversation"},
                    "report": {"type": "str"},
                    "topic": {"type": "str"},
                    "topics": list
                },
                "config": {"width": {"type": "int"}}
            },
            "nodes": [
                {"id": "plan", "kind": "node", "realizes": ["PlanTopics"],
                 "edge": {"targets": [{"node": "summaries"}]}},
                {"id": "summaries", "kind": "map",
                 "edge": {"targets": [{"node": "researcher"}, {"end": "skipped"}]},
                 "list": "topics", "item": "topic", "max_concurrency": {"config": "width"},
                 "on_item_failure": "fail_fast",
                 "body": {"kind": "call",
                          "inputs": [{"child": "topic", "source": {"from": "topic"}}],
                          "outputs": [{"child": "note", "target": {"append": "notes"}}],
                          "on_failure": "propagate",
                          "graph": {"entry": "summarize",
                                    "signature": {"inputs": {"topic": {"type": "str"}},
                                                  "outputs": {"note": {"type": "str"}}},
                                    "schema": {"state": {"note": {"type": "str"}, "topic": {"type": "str"}},
                                               "config": {}},
                                    "nodes": [{"id": "summarize", "kind": "llm_call",
                                               "edge": {"targets": [{"end": "done"}]},
                                               "model": "fixed", "system": "fixed",
                                               "request": {"text": "ready_made"},
                                               "output": {"set": "note"},
                                               "reply_retries": {"fixed": 2},
                                               "reminder": "fixed"}]}}},
                {"id": "researcher", "kind": "react_agent", "realizes": ["Researcher"],
                 "edge": {"targets": [{"end": "done"}]},
                 "inputs": [{"child": "opening", "source": {"from": "question"}}],
                 "outputs": [{"child": "answer", "target": {"set": "report"}}],
                 "on_failure": "propagate",
                 "graph": {"entry": "enter",
                           "signature": {"inputs": {"opening": {"type": "conversation"}},
                                         "outputs": {"answer": {"type": "str"}}},
                           "schema": {"state": {"answer": {"type": "str"}, "opening": {"type": "conversation"}},
                                      "config": {}},
                           "nodes": [
                             {"id": "enter", "kind": "node",
                              "edge": {"targets": [{"node": "llm"}, {"node": "answer"}]}},
                             {"id": "llm", "kind": "model_call", "join": true,
                              "edge": {"targets": [{"node": "tools"}, {"node": "answer"}]},
                              "author": {"id": "researcher"},
                              "model": "fixed", "system": "fixed", "history": "ready_made",
                              "tools": ["search", "fetch"],
                              "round_limit": {"max_rounds": {"fixed": 4}, "on_limit": "error"},
                              "reply_retries": {"fixed": 0}, "reminder": "ready_made"},
                             {"id": "tools", "kind": "tools", "edge": {"targets": [{"node": "llm"}]},
                              "tools": ["search", "fetch"]},
                             {"id": "answer", "kind": "node", "edge": {"targets": [{"end": "done"}]}}]}}
            ]
        }),
    );
}
