//! A round limit read lower than the rounds a resumed turn already holds: a
//! reply that asks for tools comes after the last allowed round, so the
//! limit handles it as it says, `End` and `Continue` included.

use br_llm_messages::{
    Conversation, StopReason, Text, ToolCallId, ToolName, ToolResult, ToolResultBlock, Turn,
    TurnId, TurnState,
};
use serde_json::json;

use crate::error::GraphError;
use crate::graph::Limit;
use crate::react::react_round_limit_support::{
    CONTINUE_REFUSAL, config, continue_limit, finished, graph, human, nid, own_turn, result_texts,
    run_graph, state_with,
};
use crate::react::round_limit::OnLimit;
use crate::react::test_support::{author, key, mock, text_step, tool_call_step};
use crate::state::State;

/// A resumed history: the agent's open turn has answered two rounds of
/// `echo` and awaits its next step.
fn two_rounds_answered() -> State {
    let mut turn = Turn::new(
        TurnId::new("t0").unwrap(),
        Some(author()),
        tool_call_step("e1", "echo", json!({})),
    );
    for (id, next) in [("e1", Some("e2")), ("e2", None)] {
        turn.push_result(ToolResult::new(
            ToolCallId::new(id).unwrap(),
            ToolName::new("echo").unwrap(),
            vec![ToolResultBlock::text(Text::new("echoed").unwrap())],
            false,
        ))
        .unwrap();
        if let Some(next) = next {
            turn.push_step(tool_call_step(next, "echo", json!({})))
                .unwrap();
        }
    }
    let mut chat = Conversation::new();
    chat.push_input(human("hi"));
    chat.push_turn(turn).unwrap();
    state_with(chat)
}

fn end_limit() -> OnLimit {
    OnLimit::End {
        node: nid("limit"),
        flag: key("limited"),
    }
}

#[tokio::test]
async fn given_a_turn_past_a_lowered_limit_when_the_reply_calls_a_tool_with_end_then_the_turn_is_closed_without_a_model_call()
 {
    let client = mock(vec![tool_call_step("c1", "echo", json!({}))]);
    let graph = graph(&client, Limit::Config(key("max_rounds")), end_limit());

    let state = finished(run_graph(&graph, config(1), two_rounds_answered()).await);

    assert_eq!(client.calls(), 1, "the turn is closed without a model call");
    assert!(state.bool(&key("limited")).unwrap());
    let turn = own_turn(&state);
    assert!(matches!(
        turn.state(),
        TurnState::Finished {
            stop_reason: StopReason::Other { reason }
        } if reason == "tool_round_limit"
    ));
    assert_eq!(
        result_texts(&turn).last(),
        Some(&(CONTINUE_REFUSAL.to_owned(), true))
    );
}

#[tokio::test]
async fn given_a_turn_past_a_lowered_limit_when_the_reply_calls_a_tool_with_continue_then_it_is_refused_and_the_model_answers()
 {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("done without tools"),
    ]);
    let graph = graph(&client, Limit::Config(key("max_rounds")), continue_limit());

    let state = finished(run_graph(&graph, config(1), two_rounds_answered()).await);

    assert_eq!(client.calls(), 2, "the model is called once more");
    assert!(state.bool(&key("limited")).unwrap());
    let turn = own_turn(&state);
    assert!(matches!(
        turn.state(),
        TurnState::Finished {
            stop_reason: StopReason::EndTurn
        }
    ));
    assert_eq!(
        result_texts(&turn).last(),
        Some(&(CONTINUE_REFUSAL.to_owned(), true))
    );
}

#[tokio::test]
async fn given_a_turn_past_a_lowered_limit_when_the_reply_after_the_refusal_still_calls_tools_with_continue_then_the_run_fails()
 {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
    ]);
    let graph = graph(&client, Limit::Config(key("max_rounds")), continue_limit());

    let failure = run_graph(&graph, config(1), two_rounds_answered())
        .await
        .err()
        .unwrap();

    assert!(matches!(
        failure.error,
        GraphError::ToolLimitReached { ref node, max_rounds: 1 } if *node == nid("llm")
    ));
    assert_eq!(
        client.calls(),
        2,
        "the limit node answered once, then the reply that still calls tools fails"
    );
    assert!(failure.checkpoint.state.bool(&key("limited")).unwrap());
}

#[tokio::test]
async fn given_a_turn_past_a_lowered_limit_when_the_reply_calls_a_tool_with_error_then_the_run_fails()
 {
    let client = mock(vec![tool_call_step("c1", "echo", json!({}))]);
    let graph = graph(&client, Limit::Config(key("max_rounds")), OnLimit::Error);

    let failure = run_graph(&graph, config(1), two_rounds_answered())
        .await
        .err()
        .unwrap();

    assert!(matches!(
        failure.error,
        GraphError::ToolLimitReached { max_rounds: 1, .. }
    ));
    assert_eq!(client.calls(), 1);
}
