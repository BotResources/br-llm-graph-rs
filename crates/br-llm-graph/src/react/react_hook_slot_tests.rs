//! Hooks inside the ReAct loop: a judgment holds its call's concurrency
//! slot, the results keep the call order, and the model reads a refusal.

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use br_llm_messages::{
    Conversation, InputId, Text, ToolResultBlock, UserBlock, UserInput, UserSource, WireMessage,
    WireUserBlock,
};
use serde_json::json;

use crate::graph::Context;
use crate::observe::NoopObserver;
use crate::react::given::Model;
use crate::react::react_agent::ReactAgent;
use crate::react::react_hook_support::*;
use crate::react::test_support::*;
use crate::run::{Outcome, control, run};
use crate::state::{Config, Kind, Value};
use crate::testkit::SeqIds;

#[tokio::test]
async fn given_a_tool_concurrency_limit_when_hooks_judge_then_each_judgment_holds_its_call_slot() {
    let calls = || {
        (1..=3)
            .map(|index| call(&format!("c{index}"), "write_file", json!({ "yields": 3 })))
            .collect::<Vec<_>>()
    };
    let limited = Arc::new(Gauge::default());
    let tool = GuardedTool::new("write_file", Some(Behaviour::Slow { yields: 3 }), &limited);
    answered(run_tools(&tool_graph(vec![tool], Some(1), false), pending(calls())).await);
    assert_eq!(
        limited.peak(),
        1,
        "a judgment and its tool run inside one slot: one at a time"
    );

    // Positive control: without a limit, the judgments overlap.
    let free = Arc::new(Gauge::default());
    let tool = GuardedTool::new("write_file", Some(Behaviour::Slow { yields: 3 }), &free);
    answered(run_tools(&tool_graph(vec![tool], None, false), pending(calls())).await);
    assert_eq!(free.peak(), 3);
}

#[tokio::test]
async fn given_judgments_that_end_out_of_order_when_calls_are_answered_then_the_results_keep_the_call_order()
 {
    let gauge = Arc::new(Gauge::default());
    let slow = GuardedTool::new("slow", Some(Behaviour::Slow { yields: 8 }), &gauge);
    let quick = GuardedTool::new("quick", Some(Behaviour::Slow { yields: 0 }), &gauge);
    let graph = tool_graph(vec![slow, quick], None, false);

    let results = answered(
        run_tools(
            &graph,
            pending(vec![
                call("c1", "slow", json!({ "refuse": true })),
                call("c2", "quick", json!({})),
                call("c3", "slow", json!({})),
            ]),
        )
        .await,
    );

    assert_eq!(
        results,
        [refused("slow", "refused slowly"), ran("quick"), ran("slow")]
    );
}

#[tokio::test]
async fn given_an_agent_whose_hook_refuses_a_call_when_it_runs_then_the_model_reads_the_refusal_and_answers()
 {
    let gauge = Arc::new(Gauge::default());
    let tool = GuardedTool::new(
        "write_file",
        Some(Behaviour::Refuse("ask the user first")),
        &gauge,
    );
    let client = mock(vec![
        tool_call_step("c1", "write_file", json!({ "path": "notes/a" })),
        text_step("I need your approval to write."),
    ]);
    let graph = ReactAgent {
        author: author(),
        model: Model::client(Arc::new(client.clone())),
        system: base(),
        history: None,
        output_mode: OutputMode::Plain,
        tools: vec![tool.clone()],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: None,
        // The hook reads it through the state it receives.
        inputs: vec![(key("log"), Kind::list(Kind::Str))],
        thread_store: None,
    }
    .graph()
    .unwrap();
    let mut history = Conversation::new();
    history
        .push_input(
            UserInput::new(
                InputId::new("i-save-my-note").unwrap(),
                UserSource::Human,
                None,
                vec![UserBlock::text(Text::new("save my note").unwrap())],
            )
            .unwrap(),
        )
        .unwrap();
    let state = graph
        .start_state([
            (key("opening"), Value::conversation(history)),
            (key("log"), Value::list(Vec::new())),
        ])
        .unwrap();
    let ctx = Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()));
    let (_sender, mut channel) = control();
    let config = Config::new(
        graph.schema(),
        BTreeMap::from([(key("base"), Value::str("Write files."))]),
    )
    .unwrap();

    let outcome = run(&graph, &config, state, None, &ctx, &mut channel)
        .await
        .map_err(|failure| failure.error)
        .unwrap();

    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(
        state.str(&key("answer")).unwrap(),
        "I need your approval to write."
    );
    assert_eq!(tool.calls(), 0);
    assert_eq!(
        tool.seen(),
        [(
            "call-0-0".to_owned(),
            json!({ "path": "notes/a" }),
            "0".to_owned(),
            "Write files.".to_owned()
        )],
        "the hook judges the call under the id the client gave, with the run's configuration"
    );
    let second = &requests(&client)[1];
    let refusal = second.messages().iter().any(|message| {
        matches!(message, WireMessage::User { content } if content.iter().any(|block| matches!(
            block,
            WireUserBlock::ToolResult(result)
                if result.is_error && result.content.iter().any(|block| matches!(
                    block,
                    ToolResultBlock::Text { text } if text.as_str() == "ask the user first"
                ))
        )))
    });
    assert!(refusal, "the model reads the refusal as an error result");
}
