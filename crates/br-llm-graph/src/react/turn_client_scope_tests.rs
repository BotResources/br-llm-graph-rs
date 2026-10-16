//! Where the client of an agent's turn is kept: in the run of the agent's
//! call, for the turn it runs. Two runs of one built graph never share one,
//! whatever their run ids; the items of a map each keep their own; a new
//! turn, and a call run again, ask the model again (its release:
//! `turn_client_release_tests.rs`).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_llm_clients_core::{Client, ClientError};
use br_llm_clients_mock::{MockClient, Reply};
use br_llm_messages::{Conversation, Entry};
use serde_json::json;

use crate::graph::{
    Always, Count, GraphBuilder, Input, ItemFailure, Map, Output, SubGraph, Target,
};
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support as duo;
use crate::react::given::{Model, ModelNode, Reads};
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::seats::GeneratorSeat;
use crate::react::test_support::*;
use crate::run::{Outcome, control, run};
use crate::state::{Config, Kind, Schema, Value};
use crate::value::EndLabel;

/// The first user input's id of `conversation`.
fn first_input(conversation: &Conversation) -> Option<String> {
    conversation.entries().iter().find_map(|entry| match entry {
        Entry::UserInput(input) => Some(input.id().as_str().to_owned()),
        Entry::Turn(_) => None,
    })
}

/// A model that gives `one` to the call whose opening is `question("one")`
/// and `other` to every other call, counting each time it is asked.
fn by_question(one: &MockClient, other: &MockClient, asks: &Arc<AtomicUsize>) -> Model<ReactAgent> {
    let (one, other): (Arc<dyn Client>, Arc<dyn Client>) =
        (Arc::new(one.clone()), Arc::new(other.clone()));
    let asks = asks.clone();
    Model::from_fn(Reads::new(), move |ctx| {
        asks.fetch_add(1, Ordering::SeqCst);
        Ok(match first_input(ctx.conversation()).as_deref() {
            Some("i-one") => one.clone(),
            _ => other.clone(),
        })
    })
}

/// A model that always gives `client`, counting each time it is asked.
fn counted<N: ModelNode>(client: &Arc<dyn Client>, asks: &Arc<AtomicUsize>) -> Model<N> {
    let (client, asks) = (client.clone(), asks.clone());
    Model::from_fn(Reads::new(), move |_| {
        asks.fetch_add(1, Ordering::SeqCst);
        Ok(client.clone())
    })
}

/// A turn of two rounds: a call of `echo`, then `answer`.
fn two_rounds(call: &str, answer: &str) -> MockClient {
    mock(vec![
        tool_call_step(call, "echo", json!({})),
        text_step(answer),
    ])
}

fn unavailable(client: &MockClient) {
    client.push_reply(
        Reply::failure(ClientError::Unavailable { retry_after: None })
            .build()
            .unwrap(),
    );
}

#[tokio::test]
async fn given_two_runs_of_one_graph_under_the_same_run_id_when_their_turns_interleave_then_each_turn_asks_its_model_once()
 {
    let (one, two) = (two_rounds("c1", "one done"), two_rounds("c2", "two done"));
    let asks = Arc::new(AtomicUsize::new(0));
    let graph = ReactAgent {
        model: by_question(&one, &two, &asks),
        tools: vec![Arc::new(YieldingEcho)],
        ..agent(&one)
    }
    .graph()
    .unwrap();
    let ctx = ctx();
    let config = agent_config(&graph);
    let start = |body: &str| {
        graph
            .start_state([(key("opening"), Value::conversation(question(body)))])
            .unwrap()
    };
    let ((_s1, mut c1), (_s2, mut c2)) = (control(), control());

    let (first, second) = futures_util::join!(
        run(&graph, &config, start("one"), None, &ctx, &mut c1),
        run(&graph, &config, start("two"), None, &ctx, &mut c2),
    );

    assert_eq!(finished(first).str(&key("answer")).unwrap(), "one done");
    assert_eq!(finished(second).str(&key("answer")).unwrap(), "two done");
    assert_eq!((one.calls(), two.calls()), (2, 2));
    assert_eq!(
        asks.load(Ordering::SeqCst),
        2,
        "one ask per turn: neither run forgets nor replaces the other's client"
    );
}

#[tokio::test]
async fn given_a_map_of_agents_when_their_turns_interleave_then_each_item_keeps_its_own_client() {
    let (one, two) = (two_rounds("c1", "A"), two_rounds("c2", "B"));
    let asks = Arc::new(AtomicUsize::new(0));
    let agent_graph = ReactAgent {
        model: by_question(&one, &two, &asks),
        tools: vec![Arc::new(YieldingEcho)],
        ..agent(&one)
    }
    .graph()
    .unwrap();
    let schema = Schema::builder()
        .state_by_name(key("questions"), Kind::list(Kind::Conversation))
        .state_by_name(key("question"), Kind::Conversation)
        .state_by_name(key("replies"), Kind::list(Kind::Str))
        .config_by_name(key("prompt"), Kind::Str)
        .build();
    let body = SubGraph::call(agent_graph)
        .input(key("opening"), Input::From(key("question")))
        .config(key("base"), Input::Config(key("prompt")))
        .output(key("answer"), Output::Append(key("replies")));
    let graph = GraphBuilder::new(schema.clone())
        .entry(nid("each"))
        .map(
            nid("each"),
            Map {
                list: key("questions"),
                item: key("question"),
                body: Box::new(body),
                max_concurrency: None,
                on_item_failure: ItemFailure::Finish,
            },
        )
        .edge(
            nid("each"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .input(key("questions"))
        .build()
        .unwrap();
    let questions = Value::list(vec![
        Value::conversation(question("one")),
        Value::conversation(question("two")),
    ]);
    let state = graph.start_state([(key("questions"), questions)]).unwrap();
    let config = Config::new(
        &schema,
        BTreeMap::from([(key("prompt"), Value::str("Be brief."))]),
    )
    .unwrap();
    let (_sender, mut channel) = control();

    let outcome = run(&graph, &config, state, None, &ctx(), &mut channel)
        .await
        .map_err(|failure| failure.error)
        .unwrap();

    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(
        state.list(&key("replies")).unwrap(),
        &[Value::str("A"), Value::str("B")]
    );
    assert_eq!((one.calls(), two.calls()), (2, 2));
    assert_eq!(asks.load(Ordering::SeqCst), 2, "one ask per item's turn");
}

#[tokio::test]
async fn given_a_generation_cut_by_the_round_limit_when_the_next_one_starts_then_it_asks_again_and_takes_the_next_client()
 {
    let first = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
    ]);
    let second = mock(vec![text_step("fine")]);
    let critic = mock(Vec::new());
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            model: Model::schedule(Arc::new(first.clone()), vec![Arc::new(second.clone())]),
            ..duo::cut_generator(&first)
        },
        duo::critic(&critic),
        Count::Fixed(1),
    )
    .graph()
    .unwrap();

    let state = duo::ended(
        duo::call(&graph, duo::question("explain")).await,
        "exhausted",
    );

    assert_eq!(state.str(&key("answer")).unwrap(), "fine");
    assert_eq!(
        (first.calls(), second.calls(), critic.calls()),
        (2, 1, 0),
        "the client kept for the cut turn does not run the next one"
    );
}

#[tokio::test]
async fn given_a_call_in_memory_that_failed_at_its_second_round_when_run_again_then_its_first_round_asks_the_model()
 {
    let client = mock(vec![tool_call_step("c1", "echo", json!({}))]);
    unavailable(&client);
    let shared: Arc<dyn Client> = Arc::new(client.clone());
    let asks = Arc::new(AtomicUsize::new(0));
    let graph = ReactAgent {
        model: counted(&shared, &asks),
        ..agent(&client)
    }
    .graph()
    .unwrap();

    assert!(call(&graph, question("hi")).await.err().is_some());
    assert_eq!(asks.load(Ordering::SeqCst), 1);
    push(&client, text_step("done"));
    let state = finished(call(&graph, question("hi")).await);

    assert_eq!(state.str(&key("answer")).unwrap(), "done");
    assert_eq!(
        asks.load(Ordering::SeqCst),
        2,
        "the same run id and trigger name the same turn, asked again"
    );
}

#[tokio::test]
async fn given_a_generator_critic_in_memory_that_failed_mid_generation_when_run_again_then_its_generator_is_asked_again()
 {
    let generator = mock(vec![tool_call_step("c1", "echo", json!({}))]);
    unavailable(&generator);
    let critic = mock(vec![duo::verdict(true, "")]);
    let shared: Arc<dyn Client> = Arc::new(generator.clone());
    let asks = Arc::new(AtomicUsize::new(0));
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            model: counted(&shared, &asks),
            tools: vec![Arc::new(EchoTool)],
            ..duo::generator(&generator)
        },
        duo::critic(&critic),
        Count::Fixed(1),
    )
    .graph()
    .unwrap();

    assert!(
        duo::call(&graph, duo::question("explain"))
            .await
            .result
            .is_err()
    );
    assert_eq!(asks.load(Ordering::SeqCst), 1);
    push(&generator, text_step("ok"));
    let state = duo::ended(
        duo::call(&graph, duo::question("explain")).await,
        "validated",
    );

    assert_eq!(state.str(&key("answer")).unwrap(), "ok");
    assert_eq!(asks.load(Ordering::SeqCst), 2);
}
