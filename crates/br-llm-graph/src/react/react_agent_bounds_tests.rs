//! The limits a ReAct agent reads from its configuration: a refused one is
//! the run's error itself, met by the edge of `enter` before any node reads
//! it, so the model is never called, even by a run whose model would answer
//! without a tool.

use std::collections::BTreeMap;

use crate::error::GraphError;
use crate::graph::Limit;
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::test_support::*;
use crate::run::{control, run};
use crate::state::{Config, Value};

/// Runs `agent` (its model a mock that answers without a tool) with
/// `name` set to `value` in its configuration, and gives the run's error,
/// asserting the model was not called.
async fn refused(
    agent: impl FnOnce(&br_llm_clients_mock::MockClient) -> ReactAgent,
    name: &str,
    value: i64,
) -> GraphError {
    let client = mock(vec![text_step("done")]);
    let graph = agent(&client).graph().unwrap();
    let config = Config::new(
        graph.schema(),
        BTreeMap::from([
            (key("base"), Value::str("You are a helpful agent.")),
            (key(name), Value::int(value)),
        ]),
    )
    .unwrap();
    let state = graph
        .start_state([(key("opening"), Value::conversation(question("hi")))])
        .unwrap();
    let (_sender, mut channel) = control();
    let failure = run(&graph, &config, state, None, &ctx(), &mut channel)
        .await
        .err()
        .unwrap();
    assert!(requests(&client).is_empty());
    failure.error
}

#[tokio::test]
async fn given_a_round_limit_below_one_when_run_then_limit_not_positive_is_the_run_error() {
    for on_limit in [
        OnLimit::Error,
        OnLimit::End {
            node: nid("limit"),
            flag: key("limited"),
        },
    ] {
        let error = refused(
            |client| ReactAgent {
                round_limit: Some(RoundLimit {
                    max_rounds: Limit::Config(key("rounds")),
                    on_limit,
                }),
                ..agent(client)
            },
            "rounds",
            0,
        )
        .await;
        assert!(
            matches!(&error, GraphError::LimitNotPositive { key: found, value: 0 } if *found == key("rounds")),
            "{error}"
        );
    }
}

#[tokio::test]
async fn given_a_tool_concurrency_below_one_when_run_then_limit_not_positive_is_the_run_error() {
    let error = refused(
        |client| ReactAgent {
            tool_concurrency: Some(Limit::Config(key("width"))),
            ..agent(client)
        },
        "width",
        -4,
    )
    .await;
    assert!(
        matches!(&error, GraphError::LimitNotPositive { key: found, value: -4 } if *found == key("width")),
        "{error}"
    );
}
