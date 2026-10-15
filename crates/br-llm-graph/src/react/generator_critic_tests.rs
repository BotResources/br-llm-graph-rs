//! The generator-critic graph: routing, budget, what each seat reads, and the
//! verdict read on a conforming client.

use std::collections::BTreeMap;

use br_llm_clients_core::{
    ClientError, OutputMode, Parameter, ParameterKind, Parameters, StructuredShape,
};
use br_llm_messages::Conversation;
use serde_json::json;

use crate::error::GraphError;
use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::seats::{CriticSeat, GeneratorSeat};
use crate::react::test_support::*;
use crate::state::{Kind, Value};

#[tokio::test]
async fn given_a_valid_first_answer_when_run_then_validated_and_nothing_is_appended() {
    let generator_client = mock(vec![text_step("draft")]);
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = duo(&generator_client, &critic_client).graph().unwrap();
    let state = ended(call(&graph, question("explain")).await, "validated");
    assert!(state.bool(&key("is_validated")).unwrap());
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(shape_of(&state, "conversation"), ["user", "writer:draft"]);
    assert_eq!(
        shape_of(&state, "generator_history"),
        ["user", "writer:draft"]
    );
    assert_eq!(state.str(&key("answer")).unwrap(), "draft");
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

#[tokio::test]
async fn given_an_answer_of_several_text_blocks_or_a_structured_one_when_run_then_answer_is_its_text()
 {
    let several = br_llm_messages::Step::new(
        vec![
            thinking_block("plan"),
            text_block("one "),
            text_block("two"),
        ],
        br_llm_messages::StopReason::EndTurn,
        None,
        None,
    )
    .unwrap();
    let structured = structured_step(json!({ "title": "Tides", "pages": 2 }));
    let book = StructuredShape::new(
        "book",
        None,
        Parameters::new(vec![
            Parameter::new("title", "", true, ParameterKind::String).unwrap(),
            Parameter::new("pages", "", true, ParameterKind::Integer).unwrap(),
        ])
        .unwrap(),
    )
    .unwrap();
    for (step, output, expected) in [
        (several, OutputMode::Plain, "one two"),
        (
            structured,
            OutputMode::Structured(book),
            r#"{"title":"Tides","pages":2}"#,
        ),
    ] {
        let generator_client = mock(vec![step]);
        let critic_client = mock(vec![verdict(true, "")]);
        let graph = GeneratorCritic::new(
            GeneratorSeat {
                output,
                ..generator(&generator_client)
            },
            critic(&critic_client),
            Count::Fixed(1),
        )
        .graph()
        .unwrap();
        let state = ended(call(&graph, question("explain")).await, "validated");
        assert_eq!(state.str(&key("answer")).unwrap(), expected);
        assert_eq!(
            requests(&critic_client)[0].messages().last(),
            Some(&user_message(&format!(
                "<generator_response>\n{expected}\n</generator_response>\n\nPlease evaluate the generator's response above."
            )))
        );
    }
}

#[tokio::test]
async fn given_a_rejection_then_a_validation_when_run_then_each_seat_reads_its_own_perspective() {
    let generator_client = mock(vec![
        thought_step("plan one", "first"),
        thought_step("plan two", "second"),
    ]);
    let critic_client = mock(vec![verdict(false, "add an example"), verdict(true, "")]);
    let graph = duo(&generator_client, &critic_client).graph().unwrap();
    let state = ended(call(&graph, question("explain")).await, "validated");
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
    let expected = [
        "user",
        "writer:first",
        "reviewer:add an example",
        "writer:second",
    ];
    assert_eq!(shape_of(&state, "conversation"), expected);
    assert_eq!(shape_of(&state, "generator_history"), expected);

    let opening = user_message("<message role=\"user\">explain</message>");
    let critique =
        user_message("<message author=\"reviewer\" role=\"agent\">add an example</message>");
    let generator_requests = requests(&generator_client);
    assert_eq!(
        system_of(&generator_requests[0]).as_deref(),
        Some("Answer the question.")
    );
    assert_eq!(
        generator_requests[0].messages(),
        std::slice::from_ref(&opening)
    );
    assert_eq!(
        generator_requests[1].messages(),
        [
            opening.clone(),
            assistant_message(vec![thinking_block("plan one"), text_block("first")]),
            critique.clone(),
        ]
    );

    let critic_requests = requests(&critic_client);
    assert_eq!(
        system_of(&critic_requests[0]).as_deref(),
        Some("Judge the answer.")
    );
    assert_eq!(
        critic_requests[0].messages(),
        [
            opening.clone(),
            user_message(
                "<generator_response>\nfirst\n</generator_response>\n\nPlease evaluate the generator's response above."
            ),
        ]
    );
    assert_eq!(
        critic_requests[1].messages(),
        [
            opening,
            assistant_message(vec![text_block("first")]),
            critique,
            user_message(
                "<generator_response>\nsecond\n</generator_response>\n\nPlease evaluate the generator's response above."
            ),
        ]
    );
}

#[tokio::test]
async fn given_every_answer_rejected_with_a_final_generation_when_run_then_exhausted_on_the_unassessed_answer()
 {
    let generator_client = mock(vec![text_step("a"), text_step("b")]);
    let critic_client = mock(vec![verdict(false, "fix it")]);
    let graph = GeneratorCritic::new(
        generator(&generator_client),
        critic(&critic_client),
        Count::Fixed(1),
    )
    .graph()
    .unwrap();
    let state = ended(call(&graph, question("explain")).await, "exhausted");
    assert!(!state.bool(&key("is_validated")).unwrap());
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
    assert_eq!(state.str(&key("answer")).unwrap(), "b");
    assert_eq!(
        shape_of(&state, "conversation"),
        ["user", "writer:a", "reviewer:fix it", "writer:b"]
    );
    assert_eq!(requests(&critic_client).len(), 1);
}

#[tokio::test]
async fn given_every_answer_rejected_without_a_final_generation_when_run_then_exhausted_on_the_rejected_answer()
 {
    let generator_client = mock(vec![text_step("a"), text_step("b")]);
    let critic_client = mock(vec![
        verdict(false, "fix it"),
        verdict(false, "still wrong"),
    ]);
    let graph = GeneratorCritic {
        final_generation: false,
        ..GeneratorCritic::new(
            generator(&generator_client),
            critic(&critic_client),
            Count::Fixed(2),
        )
    }
    .graph()
    .unwrap();
    let state = ended(call(&graph, question("explain")).await, "exhausted");
    assert!(!state.bool(&key("is_validated")).unwrap());
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
    assert_eq!(state.str(&key("answer")).unwrap(), "b");
    assert_eq!(
        shape_of(&state, "conversation"),
        [
            "user",
            "writer:a",
            "reviewer:fix it",
            "writer:b",
            "reviewer:still wrong"
        ]
    );
    assert_eq!(requests(&generator_client).len(), 2);
}

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
        assert_eq!(shape_of(&state, "conversation"), ["user", "writer:draft"]);
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
        call_with(&graph, question("explain"), Conversation::new(), config).await,
        "exhausted",
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert!(requests(&critic_client).is_empty());

    let generator_client = mock(vec![text_step("draft")]);
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = with_budget(&generator_client, &critic_client, budget(), true);
    let config = BTreeMap::from([(key("critiques"), Value::int(1))]);
    ended(
        call_with(&graph, question("explain"), Conversation::new(), config).await,
        "validated",
    );
    assert_eq!(requests(&critic_client).len(), 1);
}

#[tokio::test]
async fn given_a_negative_critique_budget_in_the_configuration_when_run_then_count_negative() {
    let generator_client = mock(vec![text_step("draft")]);
    let critic_client = mock(Vec::new());
    let graph = with_budget(
        &generator_client,
        &critic_client,
        Count::Config(key("critiques")),
        true,
    );
    let config = BTreeMap::from([(key("critiques"), Value::int(-1))]);
    let failure = call_with(&graph, question("explain"), Conversation::new(), config)
        .await
        .err()
        .unwrap();
    assert!(
        matches!(failure.error, GraphError::CountNegative { value: -1, .. }),
        "{}",
        failure.error
    );
    assert!(requests(&critic_client).is_empty());
}

#[test]
fn given_both_seats_with_one_author_when_built_then_same_author() {
    let built = GeneratorCritic::new(
        generator(&mock(Vec::new())),
        CriticSeat {
            author: writer(),
            ..critic(&mock(Vec::new()))
        },
        Count::Fixed(1),
    )
    .graph();
    assert!(matches!(built, Err(GraphError::SameAuthor { .. })));
}

#[tokio::test]
async fn given_a_block_that_does_not_read_as_the_default_verdict_when_the_critic_judges_then_structured_and_no_critique()
 {
    for malformed in [
        structured_step(json!({ "thinking": "hm", "is_valid": "yes", "message": "x" })),
        structured_step(json!({ "thinking": "hm", "is_valid": false, "message": "" })),
        structured_step(json!({ "thinking": "hm", "is_valid": false, "message": " \n" })),
        structured_step(json!({ "thinking": "hm", "is_valid": false })),
    ] {
        let generator_client = mock(vec![text_step("draft")]);
        let critic_client = mock(vec![malformed]);
        let graph = duo(&generator_client, &critic_client).graph().unwrap();
        let (state, error) = failed_in(call(&graph, question("explain")).await, "critique");
        assert!(matches!(error, GraphError::Structured { .. }), "{error}");
        assert_eq!(shape_of(&state, "conversation"), ["user", "writer:draft"]);
        assert_eq!(
            shape_of(&state, "generator_history"),
            ["user", "writer:draft"]
        );
    }
}

#[tokio::test]
async fn given_a_critic_step_without_a_structured_block_when_the_critic_judges_then_structured_and_no_critique()
 {
    // A conforming client refuses such a reply itself (next test): the step
    // comes from a client that serves it as it is.
    let critic_client = std::sync::Arc::new(LaxClient::new(vec![text_step("looks fine to me")]));
    let graph = GeneratorCritic::new(
        generator(&mock(vec![text_step("draft")])),
        CriticSeat {
            client: critic_client.clone(),
            ..critic(&mock(Vec::new()))
        },
        Count::Fixed(3),
    )
    .graph()
    .unwrap();
    let (state, error) = failed_in(call(&graph, question("explain")).await, "critique");
    assert!(
        matches!(error, GraphError::Structured { ref message } if message == "the step carries no structured block"),
        "{error}"
    );
    assert_eq!(critic_client.requests().len(), 1);
    assert_eq!(shape_of(&state, "conversation"), ["user", "writer:draft"]);
}

#[tokio::test]
async fn given_a_text_reply_to_the_verdict_request_from_a_conforming_client_then_model_and_no_critique()
 {
    let critic_client = mock(vec![text_step("looks fine to me")]);
    let graph = duo(&mock(vec![text_step("draft")]), &critic_client)
        .graph()
        .unwrap();
    let (state, error) = failed_in(call(&graph, question("explain")).await, "critique");
    assert!(
        matches!(
            error,
            GraphError::Model(ClientError::MalformedResponse { .. })
        ),
        "{error}"
    );
    assert_eq!(shape_of(&state, "conversation"), ["user", "writer:draft"]);
}
