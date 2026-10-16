//! The rule check of the generator-critic: a pass, a revision recorded and
//! answered again within the revision limit, a failure of the run as
//! returned; revisions are not critiques.

use std::collections::BTreeMap;

use br_llm_messages::{Entry, StopReason, Text, TurnState};

use crate::error::{GraphError, NodeFault};
use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::given::Reads;
use crate::react::rule_check::{RuleCheck, Ruling};
use crate::react::test_support::*;
use crate::state::{ConfigKey, Kind, Value};

/// A rule check asking for `revision` while the answer is shorter than
/// `length` characters.
fn at_least(length: usize, revision: &'static str) -> RuleCheck {
    RuleCheck::from_fn(Reads::new(), move |ctx| {
        let text: String = ctx.answer_step().text().map(Text::as_str).collect();
        Ok(if text.len() < length {
            Ruling::Revise(Text::new(revision)?)
        } else {
            Ruling::Pass
        })
    })
}

#[tokio::test]
async fn given_a_rule_check_asking_for_a_revision_when_run_then_it_is_recorded_and_the_generator_answers_again_without_the_critic()
 {
    let generator_client = mock(vec![text_step("tiny"), text_step("a full answer")]);
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = GeneratorCritic {
        rule_check: Some(at_least(8, "Write at least eight characters.")),
        ..GeneratorCritic::new(
            generator(&generator_client),
            critic(&critic_client),
            Count::Fixed(1),
        )
    }
    .graph()
    .unwrap();

    let state = ended(call(&graph, question("explain")).await, "validated");

    assert_eq!(
        shape(state.transcript()),
        [
            "user",
            "writer:tiny",
            "reviewer:Write at least eight characters.",
            "writer:a full answer",
            "reviewer:Accepted.",
        ]
    );
    let Some(Entry::Turn(revision)) = state.transcript().entries().get(2) else {
        panic!("expected the revision");
    };
    assert!(matches!(
        revision.state(),
        TurnState::Finished { stop_reason: StopReason::Other { ref reason } } if reason == "rule_check"
    ));
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
    assert_eq!(
        requests(&critic_client).len(),
        1,
        "a revision is not a critique: the budget of one still lets the critic judge"
    );
    assert_eq!(
        requests(&generator_client)[1].messages().last(),
        Some(&user_message(
            "<message author=\"reviewer\" role=\"agent\">Write at least eight characters.</message>"
        ))
    );
}

#[tokio::test]
async fn given_a_rule_check_that_always_asks_for_a_revision_when_run_then_at_most_the_revision_limit_in_a_row_then_the_answer_goes_on()
 {
    for (limit, generations) in [(Count::Fixed(2), 3), (Count::Fixed(0), 1)] {
        let generator_client = mock(vec![text_step("a"), text_step("b"), text_step("c")]);
        let critic_client = mock(vec![verdict(true, "")]);
        let graph = GeneratorCritic {
            rule_check: Some(at_least(100, "Longer.")),
            revision_limit: limit.clone(),
            ..GeneratorCritic::new(
                generator(&generator_client),
                critic(&critic_client),
                Count::Fixed(1),
            )
        }
        .graph()
        .unwrap();

        let state = ended(call(&graph, question("explain")).await, "validated");

        assert_eq!(
            state.int(&key("nb_generations")).unwrap(),
            generations,
            "{limit:?}"
        );
        assert_eq!(requests(&critic_client).len(), 1);
    }
}

#[tokio::test]
async fn given_a_revision_limit_read_from_the_configuration_when_run_then_it_bounds_the_revisions_in_a_row()
 {
    let generator_client = mock(vec![text_step("a"), text_step("b")]);
    let critic_client = mock(vec![verdict(true, "")]);
    let limit: ConfigKey<i64> = {
        let mut schema = crate::state::Schema::builder();
        schema.config("revisions").unwrap()
    };
    let graph = GeneratorCritic {
        rule_check: Some(at_least(100, "Longer.")),
        revision_limit: Count::config(&limit),
        ..GeneratorCritic::new(
            generator(&generator_client),
            critic(&critic_client),
            Count::Fixed(1),
        )
    }
    .graph()
    .unwrap();
    assert_eq!(
        graph.schema().config.get(&key("revisions")),
        Some(&Kind::Int)
    );

    let state = ended(
        call_with(
            &graph,
            question("explain"),
            Vec::new(),
            BTreeMap::from([(key("revisions"), Value::int(1))]),
        )
        .await,
        "validated",
    );

    assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
}

#[tokio::test]
async fn given_revisions_after_a_critique_when_run_then_the_revisions_in_a_row_count_again_from_zero()
 {
    // One revision in a row at most: the first generation is revised, the
    // second rejected by the critic, the third revised again.
    let generator_client = mock(vec![
        text_step("a"),
        text_step("long answer one"),
        text_step("b"),
        text_step("long answer two"),
    ]);
    let critic_client = mock(vec![verdict(false, "Another angle."), verdict(true, "")]);
    let graph = GeneratorCritic {
        rule_check: Some(at_least(5, "Longer.")),
        revision_limit: Count::Fixed(1),
        ..GeneratorCritic::new(
            generator(&generator_client),
            critic(&critic_client),
            Count::Fixed(2),
        )
    }
    .graph()
    .unwrap();

    let state = ended(call(&graph, question("explain")).await, "validated");

    assert_eq!(
        shape(state.transcript()),
        [
            "user",
            "writer:a",
            "reviewer:Longer.",
            "writer:long answer one",
            "reviewer:Another angle.",
            "writer:b",
            "reviewer:Longer.",
            "writer:long answer two",
            "reviewer:Accepted.",
        ]
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 4);
}

#[tokio::test]
async fn given_a_budget_of_zero_when_the_rule_check_asks_for_revisions_then_one_generation_with_its_revisions_and_no_critic()
 {
    let generator_client = mock(vec![text_step("a"), text_step("a longer one")]);
    let critic_client = mock(Vec::new());
    let graph = GeneratorCritic {
        rule_check: Some(at_least(5, "Longer.")),
        ..GeneratorCritic::new(
            generator(&generator_client),
            critic(&critic_client),
            Count::Fixed(0),
        )
    }
    .graph()
    .unwrap();

    let state = ended(call(&graph, question("explain")).await, "exhausted");

    assert_eq!(state.str(&key("answer")).unwrap(), "a longer one");
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
    assert!(requests(&critic_client).is_empty());
}

/// The error of a rule check of the host's.
#[derive(Debug)]
struct Forbidden(String);

impl std::fmt::Display for Forbidden {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "forbidden word: {}", self.0)
    }
}

impl std::error::Error for Forbidden {}

#[tokio::test]
async fn given_a_rule_check_that_fails_when_run_then_the_run_fails_with_its_error_as_returned() {
    let critic_client = mock(Vec::new());
    let graph = GeneratorCritic {
        rule_check: Some(RuleCheck::from_fn(Reads::new(), |_| {
            Err(Box::new(Forbidden("secret".to_owned())))
        })),
        ..GeneratorCritic::new(
            generator(&mock(vec![text_step("the secret")])),
            critic(&critic_client),
            Count::Fixed(1),
        )
    }
    .graph()
    .unwrap();

    let failure = call(&graph, question("explain"))
        .await
        .result
        .err()
        .unwrap();

    let GraphError::NodeFailed {
        node,
        source: NodeFault::Returned(error),
    } = failure.error
    else {
        panic!("expected the check to fail");
    };
    assert_eq!(node.as_str(), "check");
    assert_eq!(error.downcast_ref::<Forbidden>().unwrap().0, "secret");
    assert!(requests(&critic_client).is_empty());
}
