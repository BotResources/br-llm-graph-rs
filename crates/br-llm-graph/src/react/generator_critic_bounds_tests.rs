//! The counts and limits a generator-critic reads from its configuration:
//! a refused one is the run's error itself, met by the edge of `route`
//! before any node reads it, so no model is called.

use std::collections::BTreeMap;

use crate::error::GraphError;
use crate::graph::{Count, Limit};
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::seats::GeneratorSeat;
use crate::react::test_support::*;
use crate::state::Value;

/// Runs `graph` on a question with the int configuration `values`, and
/// gives the run's error, asserting that neither seat was called.
async fn refused(duo: GeneratorCritic, values: &[(&str, i64)]) -> GraphError {
    let generator_client = mock(vec![text_step("draft")]);
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = GeneratorCritic {
        generator: GeneratorSeat {
            model: generator(&generator_client).model,
            ..duo.generator
        },
        critic: critic(&critic_client),
        ..duo
    }
    .graph()
    .unwrap();
    let config = values
        .iter()
        .map(|(name, value)| (key(name), Value::int(*value)))
        .collect::<BTreeMap<_, _>>();
    let failure = call_with(&graph, question("explain"), Vec::new(), config)
        .await
        .result
        .err()
        .unwrap();
    assert!(requests(&generator_client).is_empty());
    assert!(requests(&critic_client).is_empty());
    failure.error
}

fn base() -> GeneratorCritic {
    let client = mock(Vec::new());
    GeneratorCritic::new(
        generator(&client),
        critic(&client),
        Count::Config(key("critiques")),
    )
}

const COUNTS: [(&str, i64); 3] = [("critiques", 2), ("revisions", 2), ("retries", 0)];

fn with_counts() -> GeneratorCritic {
    GeneratorCritic {
        revision_limit: Count::Config(key("revisions")),
        verdict_retries: Count::Config(key("retries")),
        ..base()
    }
}

/// `COUNTS` with `name` set to `value`.
fn counts_with(name: &str, value: i64) -> Vec<(&'static str, i64)> {
    COUNTS
        .iter()
        .map(|&(key, given)| (key, if key == name { value } else { given }))
        .collect()
}

#[tokio::test]
async fn given_a_negative_critique_budget_when_run_then_count_negative_is_the_run_error() {
    let error = refused(with_counts(), &counts_with("critiques", -1)).await;
    assert!(
        matches!(&error, GraphError::CountNegative { key: found, value: -1 } if *found == key("critiques")),
        "{error}"
    );
}

#[tokio::test]
async fn given_a_negative_revision_limit_when_run_then_count_negative_is_the_run_error() {
    let error = refused(with_counts(), &counts_with("revisions", -2)).await;
    assert!(
        matches!(&error, GraphError::CountNegative { key: found, value: -2 } if *found == key("revisions")),
        "{error}"
    );
}

#[tokio::test]
async fn given_negative_verdict_retries_when_run_then_count_negative_is_the_run_error() {
    let error = refused(with_counts(), &counts_with("retries", -3)).await;
    assert!(
        matches!(&error, GraphError::CountNegative { key: found, value: -3 } if *found == key("retries")),
        "{error}"
    );
}

#[tokio::test]
async fn given_a_generator_round_limit_below_one_when_run_then_limit_not_positive_is_the_run_error()
{
    let duo = base();
    let duo = GeneratorCritic {
        generator: GeneratorSeat {
            round_limit: Some(RoundLimit {
                max_rounds: Limit::Config(key("rounds")),
                on_limit: OnLimit::Error,
            }),
            ..duo.generator
        },
        ..duo
    };
    let error = refused(duo, &[("critiques", 1), ("rounds", 0)]).await;
    assert!(
        matches!(&error, GraphError::LimitNotPositive { key: found, value: 0 } if *found == key("rounds")),
        "{error}"
    );
}

#[tokio::test]
async fn given_a_generator_tool_concurrency_below_one_when_run_then_limit_not_positive_is_the_run_error()
 {
    let duo = base();
    let duo = GeneratorCritic {
        generator: GeneratorSeat {
            tool_concurrency: Some(Limit::Config(key("width"))),
            ..duo.generator
        },
        ..duo
    };
    let error = refused(duo, &[("critiques", 1), ("width", -1)]).await;
    assert!(
        matches!(&error, GraphError::LimitNotPositive { key: found, value: -1 } if *found == key("width")),
        "{error}"
    );
}
