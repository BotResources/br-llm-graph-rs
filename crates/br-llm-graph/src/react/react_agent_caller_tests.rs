//! A ReAct agent called by another graph: as a call that maps its inputs,
//! configuration and outputs, and as the body of a map.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_messages::Entry;
use serde_json::json;

use crate::graph::{
    Always, Graph, GraphBuilder, Input, ItemFailure, Limit, Map, Output, SubGraph, Target,
};
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::test_support::*;
use crate::run::{Outcome, control, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::value::EndLabel;

fn caller_schema() -> Schema {
    Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .state(key("answer"), Kind::Str)
        .state(key("questions"), Kind::list(Kind::Conversation))
        .state(key("question"), Kind::Conversation)
        .state(key("replies"), Kind::list(Kind::Str))
        .config(key("prompt"), Kind::Str)
        .build()
}

fn caller_config() -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("prompt"), Value::str("Be brief."));
    Config::new(&caller_schema(), values).unwrap()
}

async fn run_caller(graph: &Graph, state: State) -> State {
    let (_sender, mut channel) = control();
    match run(graph, &caller_config(), state, None, &ctx(), &mut channel)
        .await
        .map_err(|f| f.error)
        .unwrap()
    {
        Outcome::Finished { state, .. } => state,
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

#[tokio::test]
async fn given_the_agent_called_as_a_subgraph_when_run_then_the_caller_gets_history_and_answer() {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("done with echo"),
    ]);
    let call = SubGraph::call(agent(&client).graph().unwrap())
        .input(key("history"), Input::From(key("chat")))
        .config(key("base"), Input::Config(key("prompt")))
        .output(key("history"), Output::Set(key("chat")))
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
    let state = run_caller(&graph, state).await;
    assert_eq!(state.str(&key("answer")).unwrap(), "done with echo");
    assert_eq!(state.conversation(&key("chat")).unwrap().entries().len(), 2);
    let requests = requests(&client);
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|request| system_of(request).as_deref() == Some("Be brief."))
    );
}

#[tokio::test]
async fn given_a_pass_through_input_mapped_by_the_caller_when_a_tool_runs_then_it_reads_the_value()
{
    let client = mock(vec![
        tool_call_step("c1", "scope", json!({})),
        text_step("done"),
    ]);
    let agent = ReactAgent {
        tools: vec![Arc::new(ScopeTool)],
        inputs: vec![(key("scope"), Kind::Str)],
        ..agent(&client)
    }
    .graph()
    .unwrap();
    assert_eq!(
        agent.signature().inputs,
        BTreeMap::from([
            (key("history"), Kind::Conversation),
            (key("scope"), Kind::Str),
        ])
    );
    let call = SubGraph::call(agent)
        .input(key("history"), Input::From(key("chat")))
        .input(key("scope"), Input::From(key("folder")))
        .config(key("base"), Input::Config(key("prompt")))
        .output(key("history"), Output::Set(key("chat")));
    let schema = Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .state(key("folder"), Kind::Str)
        .config(key("prompt"), Kind::Str)
        .build();
    let graph = GraphBuilder::new(schema)
        .entry(nid("agent"))
        .subgraph(nid("agent"), call)
        .edge(
            nid("agent"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .input(key("chat"))
        .input(key("folder"))
        .build()
        .unwrap();
    let state = graph
        .start_state([
            (key("chat"), Value::conversation(question("hi"))),
            (key("folder"), Value::str("notes/2026")),
        ])
        .unwrap();
    let state = run_caller(&graph, state).await;
    let Some(Entry::Turn(turn)) = state.conversation(&key("chat")).unwrap().entries().last() else {
        panic!("expected the agent's turn");
    };
    assert_eq!(
        results_of(turn),
        [("scope".to_owned(), false, "scope is notes/2026".to_owned())]
    );
}

#[tokio::test]
async fn given_the_agent_as_a_map_body_when_run_then_each_question_gets_its_reply_in_order() {
    let client = mock(vec![
        text_step("one"),
        tool_call_step("c1", "echo", json!({})),
        text_step("two"),
    ]);
    let body = SubGraph::call(agent(&client).graph().unwrap())
        .input(key("history"), Input::From(key("question")))
        .config(key("base"), Input::Config(key("prompt")))
        .output(key("answer"), Output::Append(key("replies")));
    let map = Map {
        list: key("questions"),
        item: key("question"),
        body: Box::new(body),
        max_concurrency: Some(Limit::Fixed(NonZeroUsize::new(1).unwrap())),
        on_item_failure: ItemFailure::Finish,
    };
    let graph = GraphBuilder::new(caller_schema())
        .entry(nid("each"))
        .map(nid("each"), map)
        .edge(
            nid("each"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .input(key("questions"))
        .build()
        .unwrap();
    let questions = Value::list(vec![
        Value::conversation(question("first?")),
        Value::conversation(question("second?")),
    ]);
    let state = graph.start_state([(key("questions"), questions)]).unwrap();
    let state = run_caller(&graph, state).await;
    assert_eq!(
        state.list(&key("replies")).unwrap(),
        &[Value::str("one"), Value::str("two")]
    );
}
