//! The keys of the generator-critic's graph: its signature in memory and
//! durable, its outputs, the keys it merges, the keys it keeps for itself;
//! and the authors of its seats.

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_messages::Author;

use crate::error::GraphError;
use crate::graph::{Count, Limit};
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::given::{Prompt, Reads};
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::seats::{CriticSeat, GeneratorSeat};
use crate::react::test_support::*;
use crate::state::{ConfigKey, Kind};
use crate::testkit::TestStore;

#[test]
fn given_a_generator_critic_when_built_in_memory_and_durable_then_opening_and_thread_are_its_inputs_and_conversation_is_private()
 {
    let in_memory = duo(&mock(Vec::new()), &mock(Vec::new())).graph().unwrap();
    assert_eq!(
        in_memory.signature().inputs,
        BTreeMap::from([(key("opening"), Kind::Conversation)])
    );
    let durable = GeneratorCritic {
        thread_store: Some(Arc::new(TestStore::new())),
        ..duo(&mock(Vec::new()), &mock(Vec::new()))
    }
    .graph()
    .unwrap();
    assert_eq!(
        durable.signature().inputs,
        BTreeMap::from([
            (key("opening"), Kind::Conversation),
            (key("thread"), Kind::Str)
        ])
    );
    assert_eq!(
        durable.signature().outputs,
        BTreeMap::from([
            (key("answer"), Kind::Str),
            (key("is_validated"), Kind::Bool),
            (key("nb_generations"), Kind::Int),
            (key("thread"), Kind::Str),
        ])
    );
    for graph in [in_memory, durable] {
        for private in ["conversation", "turn_rank", "call_turns"] {
            assert!(!graph.schema().state.contains_key(&key(private)));
        }
    }
}

#[test]
fn given_a_key_the_graph_keeps_for_itself_when_given_as_an_input_then_key_collision() {
    let built = |name: &str, durable: bool| {
        GeneratorCritic {
            inputs: vec![(key(name), Kind::Bool)],
            thread_store: durable.then(|| Arc::new(TestStore::new()) as _),
            ..GeneratorCritic::new(
                GeneratorSeat {
                    round_limit: Some(RoundLimit {
                        max_rounds: limit(1),
                        on_limit: OnLimit::End {
                            node: nid("limit"),
                            flag: key("limited"),
                        },
                    }),
                    ..generator(&mock(Vec::new()))
                },
                critic(&mock(Vec::new())),
                Count::Fixed(1),
            )
        }
        .graph()
    };
    for name in [
        "opening",
        "answer",
        "is_validated",
        "nb_generations",
        "conversation",
        "turn_rank",
        "call_turns",
        "limited",
    ] {
        assert!(
            matches!(built(name, false), Err(GraphError::KeyCollision { key: ref found }) if *found == key(name)),
            "{name}"
        );
    }
    assert!(matches!(
        built("thread", true),
        Err(GraphError::KeyCollision { .. })
    ));
    assert!(
        built("thread", false).is_ok(),
        "in memory, it has no thread"
    );
}

#[test]
fn given_keys_given_twice_with_one_kind_when_built_then_each_is_declared_once() {
    let mut reads = Reads::new();
    let budget: ConfigKey<i64> = reads.config("critiques").unwrap();
    reads.read_state(&skey::<Vec<String>>("offered"));
    reads.read_state(&skey::<String>("scope"));
    let graph = GeneratorCritic {
        inputs: vec![(key("scope"), Kind::Str)],
        ..GeneratorCritic::new(
            GeneratorSeat {
                system: Prompt::from_fn(reads, move |ctx| {
                    Ok(format!("{} critiques.", ctx.config(&budget)?))
                }),
                enabled: Some(key("offered")),
                tool_concurrency: Some(Limit::Config(key("width"))),
                ..generator(&mock(Vec::new()))
            },
            critic(&mock(Vec::new())),
            Count::Config(key("critiques")),
        )
    }
    .graph()
    .unwrap();
    assert_eq!(
        graph.signature().inputs,
        BTreeMap::from([
            (key("opening"), Kind::Conversation),
            (key("offered"), Kind::list(Kind::Str)),
            (key("scope"), Kind::Str),
        ])
    );
    assert_eq!(
        graph.schema().config,
        BTreeMap::from([(key("critiques"), Kind::Int), (key("width"), Kind::Int)])
    );

    let built = GeneratorCritic::new(
        generator(&mock(Vec::new())),
        crate::react::seats::CriticSeat {
            review_request: Some(Prompt::from_config(&ckey("critiques"))),
            ..critic(&mock(Vec::new()))
        },
        Count::Config(key("critiques")),
    )
    .graph();
    assert!(
        matches!(built, Err(GraphError::KeyCollision { key: ref found }) if *found == key("critiques")),
        "a count key read as a string has another kind"
    );
}

#[test]
fn given_a_generator_flag_naming_a_key_of_the_graph_when_built_then_key_collision_or_flag_key_mismatch()
 {
    let built = |flag: &str| {
        GeneratorCritic::new(
            GeneratorSeat {
                round_limit: Some(RoundLimit {
                    max_rounds: limit(1),
                    on_limit: OnLimit::Continue {
                        node: nid("limit"),
                        flag: key(flag),
                    },
                }),
                ..generator(&mock(Vec::new()))
            },
            critic(&mock(Vec::new())),
            Count::Fixed(1),
        )
        .graph()
    };
    for flag in ["is_validated", "opening", "thread", "conversation"] {
        assert!(
            matches!(built(flag), Err(GraphError::KeyCollision { key: ref found }) if *found == key(flag)),
            "{flag}"
        );
    }
    for flag in ["answer", "nb_generations"] {
        assert!(
            matches!(built(flag), Err(GraphError::FlagKeyMismatch { key: ref found }) if *found == key(flag)),
            "{flag}"
        );
    }
}

#[test]
fn given_a_generator_critic_when_built_then_its_outputs_are_plain_data() {
    let graph = duo(&mock(Vec::new()), &mock(Vec::new())).graph().unwrap();
    let outputs: Vec<(&str, &Kind)> = graph
        .signature()
        .outputs
        .iter()
        .map(|(key, kind)| (key.as_str(), kind))
        .collect();
    assert_eq!(
        outputs,
        [
            ("answer", &Kind::Str),
            ("is_validated", &Kind::Bool),
            ("nb_generations", &Kind::Int),
        ]
    );
}

#[test]
fn given_both_seats_with_one_author_when_built_then_same_author() {
    // Two authors are one author when their ids are equal, whatever their
    // names (br-llm-messages).
    let renamed = Author::new("writer", "Editor in chief").unwrap();
    for author in [writer(), renamed] {
        let built = GeneratorCritic::new(
            generator(&mock(Vec::new())),
            CriticSeat {
                author,
                ..critic(&mock(Vec::new()))
            },
            Count::Fixed(1),
        )
        .graph();
        assert!(matches!(built, Err(GraphError::SameAuthor { .. })));
    }
    // Positive control: one name, two ids, two authors.
    let namesake = Author::new("another", "writer").unwrap();
    let built = GeneratorCritic::new(
        generator(&mock(Vec::new())),
        CriticSeat {
            author: namesake,
            ..critic(&mock(Vec::new()))
        },
        Count::Fixed(1),
    )
    .graph();
    assert!(built.is_ok());
}
