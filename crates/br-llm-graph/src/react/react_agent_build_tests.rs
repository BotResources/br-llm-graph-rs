//! The build of a ReAct agent: its signature, its configuration keys, and
//! the keys and node ids it refuses.

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
use crate::state::Kind;

#[test]
fn given_an_agent_when_built_then_its_signature_names_opening_answer_and_the_limit_flag() {
    let client = mock(Vec::new());
    let plain = agent(&client).graph().unwrap();
    let signature = plain.signature();
    assert_eq!(
        signature.inputs,
        BTreeMap::from([(key("opening"), Kind::Conversation)])
    );
    assert_eq!(
        signature.outputs,
        BTreeMap::from([(key("answer"), Kind::Str)])
    );
    assert_eq!(
        plain.schema().config,
        BTreeMap::from([(key("base"), Kind::Str)])
    );

    let flagged = ReactAgent {
        system: Prompt::from_state(&skey("persona")),
        tool_concurrency: Some(Limit::Config(key("width"))),
        ..limited(
            &client,
            2,
            OnLimit::End {
                node: nid("limit"),
                flag: key("limit_reached"),
            },
        )
    }
    .graph()
    .unwrap();
    assert_eq!(
        flagged.signature().inputs,
        BTreeMap::from([
            (key("opening"), Kind::Conversation),
            (key("persona"), Kind::Str),
        ])
    );
    assert_eq!(
        flagged.signature().outputs.get(&key("limit_reached")),
        Some(&Kind::Bool)
    );
    assert_eq!(
        flagged.schema().config,
        BTreeMap::from([(key("width"), Kind::Int)])
    );
}

#[test]
fn given_a_limit_flag_that_names_the_answer_when_built_then_flag_key_mismatch() {
    let client = mock(Vec::new());
    let built = limited(
        &client,
        1,
        OnLimit::Continue {
            node: nid("limit"),
            flag: key("answer"),
        },
    )
    .graph();
    assert!(matches!(built, Err(GraphError::FlagKeyMismatch { .. })));
}

#[test]
fn given_a_limit_flag_that_names_another_key_of_the_graph_when_built_then_key_collision() {
    let client = mock(Vec::new());
    let on_limit = || OnLimit::End {
        node: nid("limit"),
        flag: key("topic"),
    };
    let read = ReactAgent {
        system: Prompt::from_state(&skey("topic")),
        ..limited(&client, 1, on_limit())
    };
    let input = ReactAgent {
        inputs: vec![(key("topic"), Kind::Bool)],
        ..limited(&client, 1, on_limit())
    };
    for agent in [read, input] {
        assert!(matches!(
            agent.graph(),
            Err(GraphError::KeyCollision { ref key }) if *key == self::key("topic")
        ));
    }
}

#[test]
fn given_a_tool_node_or_a_limit_node_named_like_a_node_of_the_agent_when_built_then_duplicate_node()
{
    let client = mock(Vec::new());
    for reserved in ["enter", "llm", "answer"] {
        let split = ReactAgent {
            tool_nodes: vec![(nid(reserved), vec![Arc::new(EchoTool) as _])],
            ..agent(&client)
        };
        let limit = limited(
            &client,
            1,
            OnLimit::End {
                node: nid(reserved),
                flag: key("limited"),
            },
        );
        for agent in [split, limit] {
            assert!(matches!(
                agent.graph(),
                Err(GraphError::DuplicateNode { ref id }) if *id == nid(reserved)
            ));
        }
    }
    let limit_on_tools = limited(
        &client,
        1,
        OnLimit::End {
            node: nid("tools"),
            flag: key("limited"),
        },
    );
    assert!(matches!(
        limit_on_tools.graph(),
        Err(GraphError::DuplicateNode { ref id }) if *id == nid("tools")
    ));
}

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
        "persona",
        "offered",
        "limit_reached",
    ] {
        let built = flagged(vec![(key(name), Kind::Str)]);
        assert!(
            matches!(built, Err(GraphError::KeyCollision { key: ref found }) if *found == key(name)),
            "{name}"
        );
    }
    let built = flagged(vec![(key("scope"), Kind::Str), (key("scope"), Kind::Int)]);
    assert!(matches!(built, Err(GraphError::KeyCollision { .. })));
    assert!(flagged(vec![(key("scope"), Kind::Str)]).is_ok());
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
