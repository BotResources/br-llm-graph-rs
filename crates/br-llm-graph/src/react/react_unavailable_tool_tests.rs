//! A call that names a tool no tool node runs: not executed, answered with
//! an error result naming the tools offered to the call, and the turn goes
//! on, whatever else the reply calls; past the last allowed round the round
//! limit answers it first.

use std::sync::Arc;

use serde_json::json;

use crate::react::react_agent_support::*;
use crate::react::react_durable_support::{Host, call_durable, durable, opening, said, thread_of};
use crate::react::round_limit::OnLimit;
use crate::react::test_support::*;
use crate::react::test_tools::{EchoTool, TallyTool};
use crate::state::Value;
use crate::testkit::TestStore;

/// The result answering a call of `name` when `offered` are offered.
fn unavailable(name: &str, offered: &str) -> (String, bool, String) {
    (
        name.to_owned(),
        true,
        format!(
            "Tool {name} is not available: this call was not executed. The tools available are: {offered}."
        ),
    )
}

#[tokio::test]
async fn given_a_reply_that_calls_only_a_tool_no_node_runs_when_run_then_it_is_answered_and_the_model_is_called_again()
 {
    let client = mock(vec![
        tool_call_step("c1", "ghost", json!({"q": "anything"})),
        text_step("done without ghost"),
    ]);
    let done = finished(call(&agent(&client).graph().unwrap(), question("hi")).await);

    assert_eq!(done.str(&key("answer")).unwrap(), "done without ghost");
    assert_eq!(
        results_of(&last_turn(&done)),
        [unavailable("ghost", "echo")]
    );
    let requests = requests(&client);
    assert_eq!(
        requests.len(),
        2,
        "the turn goes on: the model is called again"
    );
    assert_eq!(sent_results(&requests[1]), [unavailable("ghost", "echo")]);
}

#[tokio::test]
async fn given_a_reply_mixing_known_and_unknown_calls_when_run_then_each_is_answered_in_call_order()
{
    let client = mock(vec![
        calls_step(&["ghost", "echo", "phantom"]),
        text_step("done"),
    ]);
    let done = finished(call(&agent(&client).graph().unwrap(), question("hi")).await);

    assert_eq!(
        results_of(&last_turn(&done)),
        [
            unavailable("ghost", "echo"),
            ("echo".to_owned(), false, "echoed".to_owned()),
            unavailable("phantom", "echo"),
        ]
    );
}

#[tokio::test]
async fn given_an_enabled_key_when_a_call_names_no_tool_then_only_the_offered_tools_are_named_in_declaration_order()
 {
    let client = mock(vec![calls_step(&["ghost", "echo"]), text_step("done")]);
    let mut agent = agent(&client);
    agent.tools = vec![
        Arc::new(TallyTool),
        Arc::new(EchoTool),
        Arc::new(crate::react::test_tools::ScopeTool),
    ];
    agent.enabled = Some(key("enabled"));
    agent.inputs = vec![
        (
            key("log"),
            crate::state::Kind::list(crate::state::Kind::Str),
        ),
        (key("scope"), crate::state::Kind::Str),
    ];
    let graph = agent.graph().unwrap();
    let state = graph
        .start_state([
            (key("opening"), Value::conversation(question("hi"))),
            (
                key("enabled"),
                Value::list(vec![Value::str("scope"), Value::str("tally")]),
            ),
            (key("log"), Value::list(Vec::new())),
            (key("scope"), Value::str("notes")),
        ])
        .unwrap();
    let done = finished(run_agent(&graph, state).await);

    assert_eq!(
        results_of(&last_turn(&done)),
        [
            unavailable("ghost", "tally, scope"),
            (
                "echo".to_owned(),
                true,
                "Tool echo is not enabled for this run: this call was not executed.".to_owned()
            ),
        ]
    );
}

#[tokio::test]
async fn given_a_split_of_the_tools_when_a_call_names_no_tool_then_the_first_tool_node_alone_answers_it()
 {
    let client = mock(vec![
        calls_step(&["tally", "ghost", "echo"]),
        text_step("done"),
    ]);
    let mut agent = agent(&client);
    agent.tools = vec![Arc::new(EchoTool), Arc::new(TallyTool)];
    agent.tool_nodes = vec![
        (nid("echoers"), vec![Arc::new(EchoTool) as _]),
        (nid("talliers"), vec![Arc::new(TallyTool) as _]),
    ];
    agent.inputs = vec![(
        key("log"),
        crate::state::Kind::list(crate::state::Kind::Str),
    )];
    let graph = agent.graph().unwrap();
    let state = graph
        .start_state([
            (key("opening"), Value::conversation(question("hi"))),
            (key("log"), Value::list(Vec::new())),
        ])
        .unwrap();
    let done = finished(run_agent(&graph, state).await);

    // The nodes' results in registration order, each node's in call order.
    assert_eq!(
        results_of(&last_turn(&done)),
        [
            unavailable("ghost", "echo, tally"),
            ("echo".to_owned(), false, "echoed".to_owned()),
            ("tally".to_owned(), false, "tallied".to_owned()),
        ]
    );
}

#[tokio::test]
async fn given_an_agent_without_a_tool_when_its_model_calls_one_then_its_tools_node_answers_naming_none()
 {
    let client = mock(vec![
        tool_call_step("c1", "ghost", json!({})),
        text_step("done"),
    ]);
    let mut agent = agent(&client);
    agent.tools = Vec::new();
    let done = finished(call(&agent.graph().unwrap(), question("hi")).await);

    assert_eq!(done.str(&key("answer")).unwrap(), "done");
    assert_eq!(results_of(&last_turn(&done)), [unavailable("ghost", "")]);
    assert_eq!(requests(&client).len(), 2);
}

#[tokio::test]
async fn given_a_durable_agent_when_a_call_names_no_tool_then_its_error_result_is_written_to_the_thread()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![
        tool_call_step("c1", "ghost", json!({})),
        text_step("done"),
    ]);
    let graph = durable(&client, &store).graph().unwrap();
    let host = Host::new();
    let done =
        finished(call_durable(&host, &graph, opening(&[said("i1", "hi")]), "", "run-1").await);

    let view = store.view(&thread_of(&done)).unwrap();
    let Some(br_llm_messages::Entry::Turn(turn)) = view.conversation.entries().last() else {
        panic!("the agent's turn ends the thread");
    };
    assert_eq!(results_of(turn), [unavailable("ghost", "echo")]);
}

#[tokio::test]
async fn given_the_last_allowed_round_when_a_call_names_no_tool_then_the_round_limit_answers_it_first()
 {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "ghost", json!({})),
        text_step("done"),
    ]);
    let on_limit = OnLimit::Continue {
        node: nid("limit"),
        flag: key("limited"),
    };
    let done = finished(
        call(
            &limited(&client, 1, on_limit).graph().unwrap(),
            question("hi"),
        )
        .await,
    );

    assert_eq!(
        results_of(&last_turn(&done)),
        [
            ("echo".to_owned(), false, "echoed".to_owned()),
            (
                "ghost".to_owned(),
                true,
                "Tool round limit reached (1): this call was not executed. Reply without calling tools."
                    .to_owned()
            ),
        ]
    );
    assert!(done.bool(&key("limited")).unwrap());
}

#[tokio::test]
async fn given_a_generation_that_calls_a_tool_no_node_runs_when_run_then_it_is_answered_and_the_generation_goes_on()
 {
    use crate::react::generator_critic_support as gc;

    let writer = mock(vec![
        tool_call_step("c1", "ghost", json!({})),
        text_step("the answer"),
    ]);
    let reviewer = mock(vec![gc::verdict(true, "")]);
    let graph = gc::duo(&writer, &reviewer).graph().unwrap();
    let done = gc::ended(gc::call(&graph, gc::question("explain")).await, "validated");

    assert_eq!(done.str(&key("answer")).unwrap(), "the answer");
    assert_eq!(
        sent_results(&requests(&writer)[1]),
        [unavailable("ghost", "")]
    );
}
