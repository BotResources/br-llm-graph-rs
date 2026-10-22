//! A runner tool in an agent: judged by its hook before the port is called,
//! dropped by a cancel while its call waits, and, in a durable agent called
//! as a node, keyed by the tool node below that node, its result written to
//! the agent's thread.

use std::sync::Arc;

use br_llm_messages::{Entry, ToolCallId};
use serde_json::{Value as Json, json};

use crate::graph::{Always, GraphBuilder, Input, Output, SubGraph, Target};
use crate::origin::RunId;
use crate::react::react_agent_support::{
    agent, agent_config, call, ctx, finished, last_turn, question,
};
use crate::react::react_durable_support::durable;
use crate::react::test_gate::beside;
use crate::react::test_support::results_of;
use crate::react::{Hook, HookFuture, Judgment};
use crate::run::{Outcome, control, run};
use crate::runner::RunnerTool;
use crate::runner::test_support::{call_key, calling, key, nid, object, scripted, tool};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::testkit::{TestRunners, TestStore, text_block};
use crate::value::{EndLabel, ThreadId};

/// Refuses every call whose `question` is empty.
struct AskSomething;

impl Hook for AskSomething {
    fn judge<'a>(
        &'a self,
        _id: &'a ToolCallId,
        arguments: &'a Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> HookFuture<'a> {
        let empty = arguments.get("question").and_then(Json::as_str) == Some("");
        Box::pin(async move {
            Ok(match empty {
                true => Judgment::Refuse {
                    reason: "ask a question".to_owned(),
                },
                false => Judgment::Accept,
            })
        })
    }
}

#[tokio::test]
async fn given_a_runner_tool_with_a_hook_when_it_refuses_a_call_then_the_port_is_not_called_and_the_reason_is_the_result()
 {
    let port = Arc::new(TestRunners::new());
    port.answer("documentalist", vec![text_block("d1 can answer")]);
    let client = scripted(
        calling(&[
            ("c1", "select_documents", json!({"question": ""})),
            ("c2", "select_documents", json!({"question": "tides?"})),
        ]),
        "done",
    );
    let mut agent = agent(&client);
    agent.tools = vec![Arc::new(RunnerTool {
        spec: crate::react::test_support::spec("select_documents"),
        target: crate::runner::test_support::target("documentalist"),
        port: port.clone(),
        hook: Some(Arc::new(AskSomething)),
    })];
    let done = finished(call(&agent.graph().unwrap(), question("tides?")).await);

    assert_eq!(
        results_of(&last_turn(&done)),
        [
            ("select_documents", true, "ask a question"),
            ("select_documents", false, "d1 can answer"),
        ]
        .map(|(name, error, text)| (name.to_owned(), error, text.to_owned()))
    );
    assert_eq!(
        port.keys(),
        [call_key(
            "",
            "tools",
            Some("c2"),
            &object(json!({"question": "tides?"}))
        )]
    );
}

#[tokio::test]
async fn given_a_cancel_while_a_runner_tool_call_waits_when_run_then_the_call_is_dropped_once() {
    let port = Arc::new(TestRunners::new());
    port.answer("documentalist", vec![text_block("too late")]);
    port.hold("documentalist");
    let client = scripted(
        calling(&[("c1", "select_documents", json!({"question": "tides?"}))]),
        "unused",
    );
    let mut agent = agent(&client);
    agent.tools = vec![tool("select_documents", "documentalist", &port)];
    let graph = agent.graph().unwrap();
    let state = graph
        .start_state([(key("opening"), Value::conversation(question("tides?")))])
        .unwrap();
    let (sender, mut channel) = control();
    let ctx = ctx().with_run_id(RunId::new("run-1"));
    let cancel = async {
        port.waiting(1).await;
        sender.cancel();
    };

    let (outcome, ()) = match beside(
        run(
            &graph,
            &agent_config(&graph),
            state,
            None,
            &ctx,
            &mut channel,
        ),
        cancel,
    )
    .await
    {
        Ok(both) => both,
        Err(ended) => panic!("the run ended before the call waited: {:?}", ended.is_ok()),
    };

    assert!(matches!(outcome, Ok(Outcome::Cancelled { .. })));
    assert_eq!(
        port.dropped(),
        [call_key(
            "run-1",
            "tools",
            Some("c1"),
            &object(json!({"question": "tides?"}))
        )]
    );
}

#[tokio::test]
async fn given_a_runner_tool_of_a_durable_agent_called_as_a_node_when_run_then_its_key_is_below_that_node_and_its_result_is_in_the_thread()
 {
    let store = Arc::new(TestStore::new());
    let port = Arc::new(TestRunners::new());
    port.answer("documentalist", vec![text_block("d1 can answer")]);
    let client = scripted(
        calling(&[("c1", "select_documents", json!({"question": "tides?"}))]),
        "d1 answers",
    );
    let mut agent = durable(&client, &store);
    agent.tools = vec![tool("select_documents", "documentalist", &port)];
    let schema = Schema::builder()
        .state_by_name(key("chat"), Kind::Conversation)
        .state_by_name(key("thread"), Kind::Str)
        .state_by_name(key("log"), Kind::list(Kind::Str))
        .state_by_name(key("answer"), Kind::Str)
        .config_by_name(key("prompt"), Kind::Str)
        .build();
    let call = SubGraph::call(agent.graph().unwrap())
        .input(key("opening"), Input::From(key("chat")))
        .input(key("thread"), Input::From(key("thread")))
        .input(key("log"), Input::From(key("log")))
        .config(key("base"), Input::Config(key("prompt")))
        .output(key("answer"), Output::Set(key("answer")))
        .output(key("thread"), Output::Set(key("thread")));
    let graph = GraphBuilder::new(schema.clone())
        .entry(nid("agent"))
        .subgraph(nid("agent"), call)
        .edge(
            nid("agent"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .input(key("chat"))
        .input(key("thread"))
        .input(key("log"))
        .build()
        .unwrap();
    let state = graph
        .start_state([
            (key("chat"), Value::conversation(question("tides?"))),
            (key("thread"), Value::str("")),
            (key("log"), Value::list(Vec::new())),
        ])
        .unwrap();
    let config = Config::new(&schema, [(key("prompt"), Value::str("Be brief."))].into()).unwrap();
    let (_sender, mut channel) = control();
    let outcome = run(
        &graph,
        &config,
        state,
        None,
        &ctx().with_run_id(RunId::new("run-1")),
        &mut channel,
    )
    .await
    .map_err(|failure| failure.error)
    .unwrap();
    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };

    assert_eq!(state.str(&key("answer")).unwrap(), "d1 answers");
    assert_eq!(
        port.keys(),
        [call_key(
            "run-1",
            "agent/tools",
            Some("c1"),
            &object(json!({"question": "tides?"}))
        )]
    );
    let thread = ThreadId::new(state.str(&key("thread")).unwrap()).unwrap();
    let view = store.view(&thread).unwrap();
    let Some(Entry::Turn(turn)) = view.conversation.entries().last() else {
        panic!("the agent's turn ends its thread");
    };
    assert_eq!(
        results_of(turn),
        [(
            "select_documents".to_owned(),
            false,
            "d1 can answer".to_owned()
        )]
    );
}
