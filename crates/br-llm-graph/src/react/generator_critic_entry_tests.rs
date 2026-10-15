//! Resumed calls of the generator-critic (histories given back by the host,
//! routed on how `conversation` ends), and the graph called by another one.

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_messages::{
    Author, Conversation, Text, ToolCallId, ToolName, ToolResult, ToolResultBlock, Turn, TurnId,
};
use serde_json::json;

use crate::graph::{
    Always, Count, Graph, GraphBuilder, Input, ItemFailure, Map, Output, SubGraph, Target,
};
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::test_support::*;
use crate::run::{Outcome, channel, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::value::EndLabel;

#[tokio::test]
async fn given_histories_ending_on_a_critique_with_a_budget_of_zero_when_called_again_then_exhausted_without_generating()
 {
    let mut conversation = question("explain");
    for (id, author, text) in [("g1", writer(), "a"), ("c1", reviewer(), "fix it")] {
        conversation
            .push_turn(Turn::new(
                TurnId::new(id).unwrap(),
                Some(author),
                text_step(text),
            ))
            .unwrap();
    }
    let generator_client = mock(Vec::new());
    let graph = with_budget(&generator_client, &mock(Vec::new()), Count::Fixed(0), true);
    let state = ended(
        call_with(&graph, conversation.clone(), conversation, BTreeMap::new()).await,
        "exhausted",
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(state.str(&key("answer")).unwrap(), "a");
    assert!(requests(&generator_client).is_empty());
}

#[tokio::test]
async fn given_histories_ending_on_the_last_allowed_rejection_without_a_final_generation_when_called_again_then_exhausted_without_generating()
 {
    let mut conversation = question("explain");
    for (id, author, text) in [("g1", writer(), "a"), ("c1", reviewer(), "fix it")] {
        conversation
            .push_turn(Turn::new(
                TurnId::new(id).unwrap(),
                Some(author),
                text_step(text),
            ))
            .unwrap();
    }
    let generator_client = mock(Vec::new());
    let critic_client = mock(Vec::new());
    let graph = GeneratorCritic {
        final_generation: false,
        ..GeneratorCritic::new(
            generator(&generator_client),
            critic(&critic_client),
            Count::Fixed(1),
        )
    }
    .graph()
    .unwrap();
    let state = ended(
        call_with(&graph, conversation.clone(), conversation, BTreeMap::new()).await,
        "exhausted",
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(state.str(&key("answer")).unwrap(), "a");
    assert!(requests(&generator_client).is_empty());
    assert!(requests(&critic_client).is_empty());
}

#[tokio::test]
async fn given_histories_ending_on_a_turn_that_is_not_the_critics_when_called_again_then_the_generator_answers()
 {
    let third = Author::new("observer", "observer").unwrap();
    for last in [Some(third), None] {
        let mut conversation = question("explain");
        for (id, author, text) in [
            ("g1", Some(writer()), "a"),
            ("c1", Some(reviewer()), "fix it"),
            ("x1", last, "a note"),
        ] {
            conversation
                .push_turn(Turn::new(TurnId::new(id).unwrap(), author, text_step(text)))
                .unwrap();
        }
        let generator_client = mock(vec![text_step("b")]);
        let critic_client = mock(Vec::new());
        let graph = GeneratorCritic {
            final_generation: false,
            ..GeneratorCritic::new(
                generator(&generator_client),
                critic(&critic_client),
                Count::Fixed(1),
            )
        }
        .graph()
        .unwrap();
        let state = ended(
            call_with(&graph, conversation.clone(), conversation, BTreeMap::new()).await,
            "exhausted",
        );
        assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
        assert_eq!(state.str(&key("answer")).unwrap(), "b");
        assert_eq!(requests(&generator_client).len(), 1);
        assert!(requests(&critic_client).is_empty());
    }
}

#[tokio::test]
async fn given_histories_over_a_budget_read_smaller_when_called_again_then_exhausted_without_generating()
 {
    let mut conversation = question("explain");
    for (id, author, text) in [
        ("g1", writer(), "a"),
        ("c1", reviewer(), "fix it"),
        ("g2", writer(), "b"),
        ("c2", reviewer(), "fix it again"),
    ] {
        conversation
            .push_turn(Turn::new(
                TurnId::new(id).unwrap(),
                Some(author),
                text_step(text),
            ))
            .unwrap();
    }
    let generator_client = mock(vec![text_step("c")]);
    let critic_client = mock(Vec::new());
    let graph = with_budget(
        &generator_client,
        &critic_client,
        Count::Config(key("critiques")),
        true,
    );
    let config = BTreeMap::from([(key("critiques"), Value::int(1))]);
    let state = ended(
        call_with(&graph, conversation.clone(), conversation, config).await,
        "exhausted",
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
    assert_eq!(state.str(&key("answer")).unwrap(), "b");
    assert!(requests(&generator_client).is_empty());
    assert!(requests(&critic_client).is_empty());
}

#[tokio::test]
async fn given_both_histories_ending_on_an_answer_when_called_again_then_the_critic_judges_it_first()
 {
    let mut conversation = question("explain");
    conversation
        .push_turn(Turn::new(
            TurnId::new("g1").unwrap(),
            Some(writer()),
            text_step("saved answer"),
        ))
        .unwrap();
    let mut generator_history = question("explain");
    let mut turn = Turn::new(
        TurnId::new("g1").unwrap(),
        Some(writer()),
        tool_call_step("c1", "echo", json!({})),
    );
    turn.push_result(ToolResult::new(
        ToolCallId::new("c1").unwrap(),
        ToolName::new("echo").unwrap(),
        vec![ToolResultBlock::text(Text::new("echoed").unwrap())],
        false,
    ))
    .unwrap();
    turn.push_step(text_step("saved answer")).unwrap();
    generator_history.push_turn(turn).unwrap();

    let generator_client = mock(Vec::new());
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = duo(&generator_client, &critic_client).graph().unwrap();
    let state = ended(
        call_with(&graph, conversation, generator_history, BTreeMap::new()).await,
        "validated",
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(state.str(&key("answer")).unwrap(), "saved answer");
    assert_eq!(
        shape_of(&state, "generator_history"),
        ["user", "writer:[3 items]"]
    );
    assert!(requests(&generator_client).is_empty());
}

fn caller_schema() -> Schema {
    Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .state(key("best"), Kind::Str)
        .state(key("count"), Kind::Int)
        .state(key("label"), Kind::Str)
        .state(key("questions"), Kind::list(Kind::Conversation))
        .state(key("question"), Kind::Conversation)
        .state(key("answers"), Kind::list(Kind::Str))
        .state(key("verdicts"), Kind::list(Kind::Bool))
        .build()
}

fn duo_call(graph: Arc<Graph>, conversation: &str) -> SubGraph {
    SubGraph::call(graph)
        .input(key("conversation"), Input::From(key(conversation)))
        .input(
            key("generator_history"),
            Input::Const(Value::conversation(Conversation::new())),
        )
}

async fn run_caller(graph: &Graph, state: State) -> State {
    let config = Config::new(graph.schema(), BTreeMap::new()).unwrap();
    let (_sender, mut inbox) = channel();
    match run(graph, &config, state, None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap()
    {
        Outcome::Finished { state, .. } => state,
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

#[tokio::test]
async fn given_the_duo_called_as_a_subgraph_when_run_then_the_caller_reads_the_answer_and_the_label()
 {
    let generator_client = mock(vec![text_step("a"), text_step("b")]);
    let critic_client = mock(vec![verdict(false, "fix it"), verdict(true, "")]);
    let call = duo_call(
        duo(&generator_client, &critic_client).graph().unwrap(),
        "chat",
    )
    .output(key("answer"), Output::Set(key("best")))
    .output(key("nb_generations"), Output::Set(key("count")))
    .output_end_label(Output::Set(key("label")));
    let graph = GraphBuilder::new(caller_schema())
        .entry(nid("duo"))
        .subgraph(nid("duo"), call)
        .edge(
            nid("duo"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .input(key("chat"))
        .build()
        .unwrap();
    let state = graph
        .start_state([(key("chat"), Value::conversation(question("explain")))])
        .unwrap();
    let state = run_caller(&graph, state).await;
    assert_eq!(state.str(&key("label")).unwrap(), "validated");
    assert_eq!(state.str(&key("best")).unwrap(), "b");
    assert_eq!(state.int(&key("count")).unwrap(), 2);
}

#[tokio::test]
async fn given_the_duo_as_a_map_body_when_run_then_each_question_gets_its_answer_in_order() {
    let generator_client = mock(vec![
        text_step("one"),
        text_step("two"),
        text_step("two, revised"),
    ]);
    let critic_client = mock(vec![
        verdict(true, ""),
        verdict(false, "revise"),
        verdict(true, ""),
    ]);
    let body = duo_call(
        duo(&generator_client, &critic_client).graph().unwrap(),
        "question",
    )
    .output(key("answer"), Output::Append(key("answers")))
    .output(key("is_validated"), Output::Append(key("verdicts")));
    let map = Map {
        list: key("questions"),
        item: key("question"),
        body: Box::new(body),
        max_concurrency: Some(limit(1)),
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
        state.list(&key("answers")).unwrap(),
        &[Value::str("one"), Value::str("two, revised")]
    );
    assert_eq!(
        state.list(&key("verdicts")).unwrap(),
        &[Value::bool(true), Value::bool(true)]
    );
}
