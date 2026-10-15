//! What counts as `Continue`'s node having answered in the open turn: a
//! round past the limit (0-based index `max` or more) whose results are all
//! the node's refusal, an error result holding exactly its text. A lookalike
//! in an earlier round, or a round that is not all refusals, does not count.

use std::sync::{Arc, LazyLock};

use br_llm_clients_core::ToolSpec;
use br_llm_messages::{
    AssistantBlock, Conversation, Step, StopReason, Text, ToolCall, ToolCallId, ToolName,
    ToolResult, ToolResultBlock, Turn, TurnId,
};
use serde_json::{Value as Json, json};

use crate::error::GraphError;
use crate::graph::Limit;
use crate::react::react_agent_support::{call, limited, question};
use crate::react::react_round_limit_support::{
    CONTINUE_REFUSAL, config, continue_limit, finished, graph, human, nid, own_turn, result_texts,
    run_graph, state_with,
};
use crate::react::test_support::{author, calls_step, key, mock, spec, text_step, tool_call_step};
use crate::react::tool::{Tool, ToolFuture};
use crate::state::{Config, State};

/// The refusal of a limit of `max`, as the limit node writes it.
fn refusal(max: usize) -> String {
    format!(
        "Tool round limit reached ({max}): this call was not executed. Reply without calling tools."
    )
}

/// A resumed history: the agent's open turn holds `rounds` (see
/// `rounds_turn`) and awaits its next step.
fn turn_of(rounds: &[&[(&str, bool)]]) -> State {
    let mut chat = Conversation::new();
    chat.push_input(human("hi"));
    chat.push_turn(rounds_turn(rounds)).unwrap();
    state_with(chat)
}

/// A turn of the agent holding `rounds`, each a reply calling `echo` once
/// per result and the results `(text, is_error)`, awaiting its next step.
fn rounds_turn(rounds: &[&[(&str, bool)]]) -> Turn {
    let mut turn: Option<Turn> = None;
    for (round, results) in rounds.iter().enumerate() {
        let calls: Vec<AssistantBlock> = (0..results.len())
            .map(|index| {
                AssistantBlock::ToolCall(ToolCall {
                    id: ToolCallId::new(format!("r{round}-{index}")).unwrap(),
                    name: ToolName::new("echo").unwrap(),
                    arguments: json!({}),
                })
            })
            .collect();
        let step = Step::new(calls, StopReason::AwaitingToolResults, None, None).unwrap();
        match &mut turn {
            None => turn = Some(Turn::new(TurnId::new("t0").unwrap(), Some(author()), step)),
            Some(turn) => turn.push_step(step).unwrap(),
        }
        for (index, (text, is_error)) in results.iter().enumerate() {
            let result = ToolResult::new(
                ToolCallId::new(format!("r{round}-{index}")).unwrap(),
                ToolName::new("echo").unwrap(),
                vec![ToolResultBlock::text(Text::new(*text).unwrap())],
                *is_error,
            );
            turn.as_mut().unwrap().push_result(result).unwrap();
        }
    }
    turn.unwrap()
}

/// Runs the `Continue` loop with a limit of `max` on `state`, the model
/// calling `echo` once then answering: the node answers (two model calls)
/// unless it already answered in this turn (the run fails after one).
async fn continues(state: State, max: i64) -> Result<usize, GraphError> {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("done without tools"),
    ]);
    let graph = graph(&client, Limit::Config(key("max_rounds")), continue_limit());
    match run_graph(&graph, config(max), state).await {
        Ok(outcome) => {
            let state = finished(Ok(outcome));
            assert!(state.bool(&key("limited")).unwrap());
            Ok(client.calls())
        }
        Err(failure) => Err(failure.error),
    }
}

#[tokio::test]
async fn given_a_round_past_the_limit_of_only_refusals_when_the_reply_still_calls_tools_then_the_run_fails()
 {
    // Positive control: the node answered round 1 under a limit of 1.
    let state = turn_of(&[&[("echoed", false)], &[(&refusal(1), true)]]);

    let error = continues(state, 1).await.err().unwrap();

    assert!(matches!(
        error,
        GraphError::ToolLimitReached { ref node, max_rounds: 1 } if *node == nid("llm")
    ));
}

#[tokio::test]
async fn given_rounds_past_the_limit_that_are_not_all_exact_refusals_when_the_reply_calls_tools_then_the_node_answers()
 {
    let one = refusal(1);
    let prefix = "Tool round limit reached (1): this call was not executed.";
    for round in [
        vec![(one.as_str(), false)],
        vec![(prefix, true)],
        vec![(one.as_str(), true), ("echoed", false)],
        vec![("echoed", false)],
    ] {
        let state = turn_of(&[&[("echoed", false)], &round]);

        let calls = continues(state, 1).await;

        assert_eq!(
            calls.map_err(|error| error.to_string()),
            Ok(2),
            "{round:?}: not the node's answer, so the node answers, then the model"
        );
    }
}

#[tokio::test]
async fn given_a_refusal_lookalike_in_a_round_before_the_limit_when_the_reply_calls_tools_then_the_node_answers()
 {
    // Round 0 holds the refusal of another limit, as a tool could write it.
    let state = turn_of(&[&[(&refusal(9), true)]]);

    assert_eq!(
        continues(state, 1).await.map_err(|error| error.to_string()),
        Ok(2)
    );
}

#[tokio::test]
async fn given_a_refusal_under_an_earlier_lower_limit_when_the_turn_resumes_under_a_higher_one_then_the_node_answers_at_the_new_limit()
 {
    // R3b: the node answered round 1 under a limit of 1; the turn resumes
    // under a limit of 3.
    let state = turn_of(&[&[("echoed", false)], &[(&refusal(1), true)]]);
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
        text_step("done"),
    ]);
    let graph = graph(&client, Limit::Config(key("max_rounds")), continue_limit());

    let state = finished(run_graph(&graph, config(3), state).await);

    assert_eq!(
        client.calls(),
        3,
        "a tool round, the node's answer, then the reply"
    );
    let texts = result_texts(&own_turn(&state));
    assert_eq!(
        texts[2],
        ("echoed".to_owned(), false),
        "round 2 ran its tool"
    );
    assert_eq!(texts[3], (refusal(3), true), "round 3 is the node's answer");
}

/// Fails every call with the refusal text of a limit of 9.
struct Spoofing;

impl Tool for Spoofing {
    fn spec(&self) -> &ToolSpec {
        static SPEC: LazyLock<ToolSpec> = LazyLock::new(|| spec("fail"));
        &SPEC
    }

    fn call<'a>(
        &'a self,
        _arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async { Err(refusal(9).into()) })
    }
}

#[tokio::test]
async fn given_a_tool_whose_error_is_the_refusal_text_when_the_limit_is_reached_then_continue_still_answers()
 {
    // R3b's probe through a `ReactAgent`: `fail` answers round 0 with the
    // refusal's text, then a reply asks for `echo` past the limit of 1.
    let client = mock(vec![
        calls_step(&["fail"]),
        calls_step(&["echo"]),
        text_step("done"),
    ]);
    let mut agent = limited(&client, 1, continue_limit());
    agent.tools.push(Arc::new(Spoofing));
    let graph = agent.graph().unwrap();

    let result = call(&graph, question("go")).await;

    assert!(
        result.is_ok(),
        "the run failed with `{}` before the limit node answered",
        result.err().unwrap().error
    );
    assert_eq!(client.calls(), 3);
}

#[test]
fn given_the_refusal_this_file_writes_when_compared_then_it_is_the_nodes() {
    assert_eq!(refusal(1), CONTINUE_REFUSAL);
}

#[tokio::test]
async fn given_a_refusal_in_a_finished_turn_when_a_new_turn_reaches_the_limit_then_the_node_answers()
 {
    // The node's answer counts in the agent's open turn only: an earlier,
    // finished turn holding it is another turn.
    let mut earlier = rounds_turn(&[&[("echoed", false)], &[(&refusal(1), true)]]);
    earlier.push_step(text_step("earlier answer")).unwrap();
    let mut chat = Conversation::new();
    chat.push_input(human("hi"));
    chat.push_turn(earlier).unwrap();
    chat.push_input(human("again"));
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
        text_step("done"),
    ]);
    let graph = graph(&client, Limit::Config(key("max_rounds")), continue_limit());

    let state = finished(run_graph(&graph, config(1), state_with(chat)).await);

    assert_eq!(
        client.calls(),
        3,
        "a tool round, the node's answer, then the reply"
    );
    assert!(state.bool(&key("limited")).unwrap());
    assert_eq!(
        result_texts(&own_turn(&state)),
        [("echoed".to_owned(), false), (refusal(1), true)]
    );
}
