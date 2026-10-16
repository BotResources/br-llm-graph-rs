//! What the rule check reads: its context (the answer's final step, the
//! generation, the budget, the keys it declares), and never an empty answer.

use std::sync::{Arc, Mutex};

use br_llm_messages::{StopReason, Text};

use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::given::Reads;
use crate::react::rule_check::{RuleCheck, Ruling};
use crate::react::test_support::*;
use crate::state::{Kind, StateKey, Value};

#[tokio::test]
async fn given_a_rule_check_when_it_judges_then_its_context_holds_the_answer_the_generation_the_budget_and_its_keys()
 {
    // A client that serves both a text and a structured answer as they are.
    let generator_client = Arc::new(LaxClient::new(vec![
        text_step("draft"),
        structured_step(serde_json::json!({ "done": true })),
    ]));
    let critic_client = mock(vec![verdict(true, "")]);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = seen.clone();
    let mut reads = Reads::new();
    let style: StateKey<String> = reads.state("style").unwrap();
    let graph = GeneratorCritic {
        // `style` is also an extra input, for the tools: declared once.
        inputs: vec![(key("style"), Kind::Str)],
        rule_check: Some(RuleCheck::from_fn(reads, move |ctx| {
            let answer = ctx.answer_step();
            kept.lock().unwrap().push((
                answer.text().map(Text::as_str).collect::<String>(),
                answer.structured().next().cloned(),
                matches!(answer.stop_reason(), StopReason::EndTurn),
                ctx.generation(),
                ctx.budget(),
                ctx.get(&style)?.to_owned(),
                ctx.conversation().entries().len(),
            ));
            Ok(if ctx.generation() == 1 {
                Ruling::Revise(Text::new("Answer as data.")?)
            } else {
                Ruling::Pass
            })
        })),
        ..GeneratorCritic::new(
            crate::react::seats::GeneratorSeat {
                model: crate::react::given::Model::client(generator_client),
                ..generator(&mock(Vec::new()))
            },
            critic(&critic_client),
            Count::Fixed(3),
        )
    }
    .graph()
    .unwrap();

    ended(
        start(&graph, vec![(key("style"), Value::str("terse"))]).await,
        "validated",
    );

    assert_eq!(
        *seen.lock().unwrap(),
        [
            ("draft".to_owned(), None, true, 1, 3, "terse".to_owned(), 2),
            (
                String::new(),
                Some(serde_json::json!({ "done": true })),
                true,
                2,
                3,
                "terse".to_owned(),
                4
            ),
        ]
    );
}

#[tokio::test]
async fn given_an_empty_answer_when_checked_then_the_rule_check_does_not_read_it() {
    let calls = Arc::new(Mutex::new(0));
    let counted = calls.clone();
    let generator_client = Arc::new(LaxClient::new(vec![
        thinking_only_step(),
        text_step("full"),
    ]));
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = GeneratorCritic {
        rule_check: Some(RuleCheck::from_fn(Reads::new(), move |_| {
            *counted.lock().unwrap() += 1;
            Ok(Ruling::Pass)
        })),
        ..GeneratorCritic::new(
            crate::react::seats::GeneratorSeat {
                model: crate::react::given::Model::client(generator_client),
                ..generator(&mock(Vec::new()))
            },
            critic(&critic_client),
            Count::Fixed(2),
        )
    }
    .graph()
    .unwrap();

    let state = ended(call(&graph, question("explain")).await, "validated");

    assert_eq!(*calls.lock().unwrap(), 1, "only the full answer is checked");
    assert_eq!(shape(state.transcript())[2], empty_critique());
}
