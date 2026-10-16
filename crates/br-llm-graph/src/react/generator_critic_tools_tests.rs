//! The generator's tools in the generator-critic: their traffic, the
//! `enabled` key, the pass-through inputs and the keys they may not take.

use std::sync::Arc;

use serde_json::json;

use crate::error::GraphError;
use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::helpers::last_turn_by_author as last_turn_of;

use crate::react::generator_critic_support::writer as generator_author;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::seats::GeneratorSeat;
use crate::react::test_support::*;
use crate::state::{Kind, Value};

#[tokio::test]
async fn given_a_generator_with_a_tool_when_run_then_its_tool_traffic_is_recorded_in_the_conversation()
 {
    let generator_client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("answer one"),
        tool_call_step("c2", "echo", json!({})),
        text_step("answer two"),
    ]);
    let critic_client = mock(vec![verdict(false, "go further"), verdict(true, "")]);
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            tools: vec![Arc::new(EchoTool)],
            round_limit: Some(RoundLimit {
                max_rounds: limit(1),
                on_limit: OnLimit::Error,
            }),
            ..generator(&generator_client)
        },
        critic(&critic_client),
        Count::Fixed(3),
    )
    .graph()
    .unwrap();
    let state = ended(call(&graph, question("explain")).await, "validated");
    assert_eq!(
        shape(state.transcript()),
        [
            "user",
            "writer:[3 items] answer one",
            "reviewer:go further",
            "writer:[3 items] answer two",
            "reviewer:Accepted.",
        ]
    );
    assert_eq!(state.str(&key("answer")).unwrap(), "answer two");
}

#[test]
fn given_a_generator_limit_flag_that_names_is_validated_when_built_then_key_collision() {
    let client = mock(Vec::new());
    let built = GeneratorCritic::new(
        GeneratorSeat {
            round_limit: Some(RoundLimit {
                max_rounds: limit(1),
                on_limit: OnLimit::End {
                    node: nid("limit"),
                    flag: key("is_validated"),
                },
            }),
            ..cut_generator(&client)
        },
        critic(&client),
        Count::Fixed(1),
    )
    .graph();
    assert!(matches!(
        built,
        Err(GraphError::KeyCollision { ref key }) if *key == self::key("is_validated")
    ));
}

#[tokio::test]
async fn given_a_generator_with_an_enabled_subset_when_run_then_only_it_is_offered_and_another_call_is_refused()
 {
    let generator_client = mock(vec![
        tool_call_step("c1", "write_file", json!({})),
        text_step("answer"),
    ]);
    let critic_client = mock(vec![verdict(true, "")]);
    let writer = Arc::new(WriteFileTool::default());
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            tools: vec![Arc::new(EchoTool), writer.clone()],
            enabled: Some(key("offered")),
            ..generator(&generator_client)
        },
        critic(&critic_client),
        Count::Fixed(1),
    )
    .graph()
    .unwrap();
    assert_eq!(
        graph.signature().inputs.get(&key("offered")),
        Some(&Kind::list(Kind::Str))
    );
    let state = ended(
        start(
            &graph,
            vec![(key("offered"), Value::list(vec![Value::str("echo")]))],
        )
        .await,
        "validated",
    );
    let Some(turn) = last_turn_of(state.transcript(), &generator_author()) else {
        panic!("expected the generator's turn");
    };
    assert_eq!(
        results_of(turn),
        [(
            "write_file".to_owned(),
            true,
            "Tool write_file is not enabled for this run: this call was not executed.".to_owned()
        )]
    );
    assert_eq!(writer.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    let requests = requests(&generator_client);
    assert_eq!(requests.len(), 2);
    for request in requests {
        assert_eq!(tool_names(&request), ["echo"]);
    }
}

#[test]
fn given_a_generator_enabled_key_that_names_a_key_of_the_graph_when_built_then_key_collision() {
    let built = GeneratorCritic::new(
        GeneratorSeat {
            enabled: Some(key("answer")),
            ..generator(&mock(Vec::new()))
        },
        critic(&mock(Vec::new())),
        Count::Fixed(1),
    )
    .graph();
    assert!(matches!(built, Err(GraphError::KeyCollision { .. })));
}

#[tokio::test]
async fn given_a_pass_through_input_when_the_generator_calls_a_tool_then_the_tool_reads_the_value()
{
    let generator_client = mock(vec![
        tool_call_step("c1", "scope", json!({})),
        text_step("answer"),
    ]);
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = GeneratorCritic {
        inputs: vec![(key("scope"), Kind::Str)],
        ..GeneratorCritic::new(
            GeneratorSeat {
                tools: vec![Arc::new(ScopeTool)],
                ..generator(&generator_client)
            },
            critic(&critic_client),
            Count::Fixed(1),
        )
    }
    .graph()
    .unwrap();
    assert_eq!(
        graph.signature().inputs.get(&key("scope")),
        Some(&Kind::Str)
    );
    let state = ended(
        start(&graph, vec![(key("scope"), Value::str("notes/2026"))]).await,
        "validated",
    );
    let Some(turn) = last_turn_of(state.transcript(), &generator_author()) else {
        panic!("expected the generator's turn");
    };
    assert_eq!(
        results_of(turn),
        [("scope".to_owned(), false, "scope is notes/2026".to_owned())]
    );
}

#[test]
fn given_a_pass_through_input_that_names_a_key_of_the_graph_when_built_then_key_collision() {
    for name in [
        "opening",
        "conversation",
        "turn_rank",
        "call_turns",
        "answer",
        "is_validated",
        "nb_generations",
    ] {
        let built = GeneratorCritic {
            inputs: vec![(key(name), Kind::Str)],
            ..duo(&mock(Vec::new()), &mock(Vec::new()))
        }
        .graph();
        assert!(
            matches!(built, Err(GraphError::KeyCollision { key: ref found }) if *found == key(name)),
            "{name}"
        );
    }
}

#[tokio::test]
async fn given_a_generator_round_limit_that_continues_when_a_reply_calls_tools_past_it_then_the_limit_node_goes_back_to_the_generator()
 {
    let generator_client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
        text_step("answer"),
    ]);
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            tools: vec![Arc::new(EchoTool)],
            round_limit: Some(RoundLimit {
                max_rounds: limit(1),
                on_limit: OnLimit::Continue {
                    node: nid("limit"),
                    flag: key("limited"),
                },
            }),
            ..generator(&generator_client)
        },
        critic(&critic_client),
        Count::Fixed(1),
    )
    .graph()
    .unwrap();

    let state = ended(call(&graph, question("explain")).await, "validated");

    assert_eq!(state.str(&key("answer")).unwrap(), "answer");
    assert!(state.bool(&key("limited")).unwrap());
    assert_eq!(generator_client.calls(), 3);
}
