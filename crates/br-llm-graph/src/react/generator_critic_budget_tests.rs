//! The critique budget of the generator-critic: zero gives one generation
//! and no critic call; a budget may be read from the configuration (its
//! refusal is pinned with the other bounds, `generator_critic_bounds_tests`).

use std::collections::BTreeMap;

use crate::graph::Count;
use crate::react::generator_critic_support::*;
use crate::react::test_support::*;
use crate::state::{Kind, Value};

#[tokio::test]
async fn given_a_critique_budget_of_zero_when_run_then_one_generation_no_critic_and_exhausted() {
    for final_generation in [true, false] {
        let generator_client = mock(vec![text_step("draft")]);
        let critic_client = mock(Vec::new());
        let graph = with_budget(
            &generator_client,
            &critic_client,
            Count::Fixed(0),
            final_generation,
        );
        let state = ended(call(&graph, question("explain")).await, "exhausted");
        assert!(!state.bool(&key("is_validated")).unwrap());
        assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
        assert_eq!(state.str(&key("answer")).unwrap(), "draft");
        assert_eq!(shape(state.transcript()), ["user", "writer:draft"]);
        assert_eq!(requests(&generator_client).len(), 1);
        assert!(requests(&critic_client).is_empty());
    }
}

#[tokio::test]
async fn given_a_critique_budget_read_from_the_configuration_when_run_then_zero_skips_the_critic_and_one_calls_it()
 {
    let budget = || Count::Config(key("critiques"));
    let generator_client = mock(vec![text_step("draft")]);
    let critic_client = mock(Vec::new());
    let graph = with_budget(&generator_client, &critic_client, budget(), true);
    assert_eq!(
        graph.schema().config.get(&key("critiques")),
        Some(&Kind::Int)
    );
    let config = BTreeMap::from([(key("critiques"), Value::int(0))]);
    let state = ended(
        call_with(&graph, question("explain"), Vec::new(), config).await,
        "exhausted",
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert!(requests(&critic_client).is_empty());

    let generator_client = mock(vec![text_step("draft")]);
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = with_budget(&generator_client, &critic_client, budget(), true);
    let config = BTreeMap::from([(key("critiques"), Value::int(1))]);
    ended(
        call_with(&graph, question("explain"), Vec::new(), config).await,
        "validated",
    );
    assert_eq!(requests(&critic_client).len(), 1);
}
