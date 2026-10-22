//! What a tool is called with besides its arguments: the id and the name of
//! the call, and the origin of the tool node that runs it, below the node
//! that calls the agent's graph.

use std::sync::{Arc, LazyLock, Mutex};

use br_llm_clients_core::ToolSpec;
use br_llm_messages::{ToolCallId, ToolName, ToolResultBlock};
use serde_json::{Value as Json, json};

use crate::graph::{Always, GraphBuilder, Input, Output, SubGraph, Target};
use crate::origin::{OccurrenceKey, Origin, RunId};
use crate::react::react_agent_support::*;
use crate::react::test_support::*;
use crate::react::tool::{Tool, ToolContext, ToolFuture, ToolOutput};
use crate::run::{Outcome, control, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::value::EndLabel;

/// Keeps what each of its calls was called with.
#[derive(Default)]
struct Witness {
    calls: Mutex<Vec<(ToolContext, Json)>>,
}

impl Tool for Witness {
    fn spec(&self) -> &ToolSpec {
        static SPEC: LazyLock<ToolSpec> = LazyLock::new(|| spec("witness"));
        &SPEC
    }

    fn call<'a>(
        &'a self,
        call: &'a ToolContext,
        arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        self.calls.lock().unwrap().push((call.clone(), arguments));
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                br_llm_messages::Text::new("seen").unwrap(),
            )]))
        })
    }
}

fn caller_schema() -> Schema {
    Schema::builder()
        .state_by_name(key("chat"), Kind::Conversation)
        .state_by_name(key("answer"), Kind::Str)
        .config_by_name(key("prompt"), Kind::Str)
        .build()
}

#[tokio::test]
async fn given_an_agent_called_as_a_node_when_its_tool_runs_then_it_gets_the_call_id_name_and_the_tool_node_origin()
 {
    let witness = Arc::new(Witness::default());
    let client = mock_at(br_llm_clients_core::ReasoningLevel::Medium);
    client.push_reply(
        br_llm_clients_mock::Reply::success(tool_call_step("c7", "witness", json!({"q": "tides"})))
            .build()
            .unwrap(),
    );
    push(&client, text_step("done"));
    let mut agent = agent(&client);
    agent.tools = vec![witness.clone()];
    let call = SubGraph::call(agent.graph().unwrap())
        .input(key("opening"), Input::From(key("chat")))
        .config(key("base"), Input::Config(key("prompt")))
        .output(key("answer"), Output::Set(key("answer")));
    let graph = GraphBuilder::new(caller_schema())
        .entry(nid("agent"))
        .subgraph(nid("agent"), call)
        .edge(
            nid("agent"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .input(key("chat"))
        .build()
        .unwrap();
    let state = graph
        .start_state([(key("chat"), Value::conversation(question("hi")))])
        .unwrap();
    let config = Config::new(
        &caller_schema(),
        [(key("prompt"), Value::str("Be brief."))].into(),
    )
    .unwrap();
    let ctx = ctx().with_run_id(RunId::new("run-7"));
    let (_sender, mut channel) = control();
    let outcome = run(&graph, &config, state, None, &ctx, &mut channel)
        .await
        .map_err(|failure| failure.error)
        .unwrap();
    assert!(matches!(outcome, Outcome::Finished { .. }));

    let calls = witness.calls.lock().unwrap().clone();
    assert_eq!(
        calls,
        [(
            ToolContext::new(
                ToolCallId::new("c7").unwrap(),
                ToolName::new("witness").unwrap(),
                Origin {
                    run: RunId::new("run-7"),
                    occurrence: OccurrenceKey::parse("agent/tools").unwrap(),
                },
            ),
            json!({"q": "tides"}),
        )]
    );
}

#[tokio::test]
async fn given_a_split_of_the_tools_when_a_tool_runs_then_its_origin_is_the_tool_node_that_runs_it()
{
    let witness = Arc::new(Witness::default());
    let client = mock(vec![calls_step(&["echo", "witness"]), text_step("done")]);
    let mut agent = agent(&client);
    agent.tools = vec![
        Arc::new(crate::react::test_tools::EchoTool),
        witness.clone(),
    ];
    agent.tool_nodes = vec![
        (
            nid("echoers"),
            vec![Arc::new(crate::react::test_tools::EchoTool) as _],
        ),
        (nid("witnesses"), vec![witness.clone() as _]),
    ];
    finished(call(&agent.graph().unwrap(), question("hi")).await);

    let calls = witness.calls.lock().unwrap().clone();
    let [(context, _)] = calls.as_slice() else {
        panic!("one call of the witness, got {calls:?}");
    };
    assert_eq!(context.origin.occurrence.to_string(), "witnesses");
    assert_eq!(context.name.as_str(), "witness");
}
