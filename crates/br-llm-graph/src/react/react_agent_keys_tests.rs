//! The keys a ReAct agent declares for what its caller and its runner give:
//! the keys its functions read, its enabled key and its extra inputs, each
//! declared once with one kind, and refused when the graph already has it.

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::Client;

use crate::error::GraphError;
use crate::graph::Limit;
use crate::react::given::{History, Model, Prompt, Reads};
use crate::react::helpers::wire;
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::round_limit::OnLimit;
use crate::react::test_support::*;
use crate::state::{ConfigKey, Kind};

#[test]
fn given_an_enabled_key_that_names_a_key_of_the_graph_when_built_then_key_collision() {
    for name in ["opening", "answer", "conversation", "turn_rank"] {
        let built = ReactAgent {
            enabled: Some(key(name)),
            ..agent(&mock(Vec::new()))
        }
        .graph();
        assert!(
            matches!(built, Err(GraphError::KeyCollision { key: ref found }) if *found == key(name))
        );
    }
}

#[test]
fn given_a_pass_through_input_that_names_a_key_of_the_graph_when_built_then_key_collision() {
    let flagged = |inputs: Vec<(crate::value::Key, Kind)>| {
        ReactAgent {
            system: Prompt::from_state(&skey("persona")),
            enabled: Some(key("offered")),
            inputs,
            ..limited(
                &mock(Vec::new()),
                1,
                OnLimit::End {
                    node: nid("limit"),
                    flag: key("limit_reached"),
                },
            )
        }
        .graph()
    };
    for name in [
        "opening",
        "answer",
        "conversation",
        "turn_rank",
        "offered",
        "limit_reached",
    ] {
        let built = flagged(vec![(key(name), Kind::Str)]);
        assert!(
            matches!(built, Err(GraphError::KeyCollision { key: ref found }) if *found == key(name)),
            "{name}"
        );
    }
    let built = flagged(vec![(key("persona"), Kind::Int)]);
    assert!(
        matches!(built, Err(GraphError::KeyCollision { key: ref found }) if *found == key("persona"))
    );
    let built = flagged(vec![(key("scope"), Kind::Str), (key("scope"), Kind::Int)]);
    assert!(matches!(built, Err(GraphError::KeyCollision { .. })));
    assert!(flagged(vec![(key("scope"), Kind::Str)]).is_ok());
}

#[test]
fn given_an_extra_input_that_names_a_key_a_function_reads_or_the_enabled_key_with_its_kind_when_built_then_one_input()
 {
    let graph = ReactAgent {
        system: Prompt::from_state(&skey("persona")),
        enabled: Some(key("offered")),
        inputs: vec![
            (key("persona"), Kind::Str),
            (key("offered"), Kind::list(Kind::Str)),
            (key("persona"), Kind::Str),
        ],
        ..agent(&mock(Vec::new()))
    }
    .graph()
    .unwrap();
    assert_eq!(
        graph.signature().inputs,
        BTreeMap::from([
            (key("opening"), Kind::Conversation),
            (key("persona"), Kind::Str),
            (key("offered"), Kind::list(Kind::Str)),
        ])
    );
}

#[test]
fn given_a_key_read_by_several_functions_when_built_then_it_is_declared_once() {
    let client = mock(Vec::new());
    let both = || {
        let mut reads = Reads::new();
        reads.read_state(&skey::<String>("persona"));
        reads.read_config(&ckey::<String>("base"));
        reads.read_state(&skey::<String>("persona"));
        reads
    };
    let model_client: Arc<dyn Client> = Arc::new(client.clone());
    let graph = ReactAgent {
        model: Model::from_fn(both(), move |_| Ok(model_client.clone())),
        system: Prompt::from_fn(both(), |_| Ok(String::new())),
        history: Some(History::from_fn(both(), |ctx| {
            Ok(wire(ctx.conversation(), &author())?)
        })),
        ..agent(&client)
    }
    .graph()
    .unwrap();
    assert_eq!(
        graph.signature().inputs,
        BTreeMap::from([
            (key("opening"), Kind::Conversation),
            (key("persona"), Kind::Str),
        ])
    );
    assert_eq!(
        graph.schema().config,
        BTreeMap::from([(key("base"), Kind::Str)])
    );
}

#[test]
fn given_a_function_that_reads_a_key_of_the_graph_when_built_then_key_collision() {
    let flagged = |system: Prompt<ReactAgent>| {
        ReactAgent {
            system,
            tool_concurrency: Some(Limit::Config(key("width"))),
            ..limited(
                &mock(Vec::new()),
                1,
                OnLimit::End {
                    node: nid("limit"),
                    flag: key("limit_reached"),
                },
            )
        }
        .graph()
    };
    for name in [
        "opening",
        "answer",
        "conversation",
        "turn_rank",
        "limit_reached",
    ] {
        let built = flagged(Prompt::from_state(&skey(name)));
        assert!(
            matches!(built, Err(GraphError::KeyCollision { key: ref found }) if *found == key(name)),
            "{name}"
        );
    }
    let built = flagged(Prompt::from_config(&ckey("width")));
    assert!(
        matches!(built, Err(GraphError::KeyCollision { key: ref found }) if *found == key("width"))
    );
}

#[test]
fn given_a_function_that_reads_the_key_of_a_limit_with_its_kind_when_built_then_one_configuration_key()
 {
    let mut reads = Reads::new();
    let width: ConfigKey<i64> = reads.config("width").unwrap();
    let graph = ReactAgent {
        system: Prompt::from_fn(reads, move |ctx| {
            Ok(format!("At most {} calls at once.", ctx.config(&width)?))
        }),
        tool_concurrency: Some(Limit::Config(key("width"))),
        ..agent(&mock(Vec::new()))
    }
    .graph()
    .unwrap();
    assert_eq!(
        graph.schema().config,
        BTreeMap::from([(key("width"), Kind::Int)])
    );
}
